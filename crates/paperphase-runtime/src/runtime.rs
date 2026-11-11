use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use reqwest::{Url, blocking::Client, redirect::Policy};
use serde::Serialize;
use wasmtime::{
    Caller, Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    Strategy,
};
use wasmtime::{Result, ensure, error::Context, format_err as anyhow};

use crate::Dataset;
use paperphase_core::Publication;

const ABI: &str = "paperphase_v0";
const MAX_URL_BYTES: usize = 8192;
const MAX_SETTINGS_BYTES: usize = 16 * 1024;

/// Initial harness budgets. These are not measured production settings.
#[derive(Clone, Debug)]
pub struct Limits {
    pub memory_bytes: usize,
    pub fuel: u64,
    pub max_module_bytes: usize,
    pub max_response_bytes: usize,
    pub max_output_bytes: usize,
    pub max_requests: u32,
    pub request_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 32 * 1024 * 1024,
            fuel: 10_000_000,
            max_module_bytes: 2 * 1024 * 1024,
            max_response_bytes: 64 * 1024,
            max_output_bytes: 64 * 1024,
            max_requests: 4,
            request_timeout: Duration::from_secs(3),
        }
    }
}

#[derive(Clone)]
pub struct RunConfig {
    source_url: Url,
    allowed_origin: String,
    settings: Vec<u8>,
}
impl std::fmt::Debug for RunConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunConfig")
            .field("allowed_origin", &self.allowed_origin)
            .field("settings_bytes", &self.settings.len())
            .finish_non_exhaustive()
    }
}

impl RunConfig {
    /// The caller grants exactly the configured origin, including scheme/port.
    pub fn new(source_url: &str) -> Result<Self> {
        ensure!(source_url.len() <= MAX_URL_BYTES, "source URL is too long");
        let source_url = Url::parse(source_url).context("invalid source URL")?;
        validate_url(&source_url)?;
        let allowed_origin = source_url.origin().ascii_serialization();
        Ok(Self {
            source_url,
            allowed_origin,
            settings: b"{}".to_vec(),
        })
    }
    pub fn with_settings(mut self, settings: &paperphase_core::SettingsValues) -> Result<Self> {
        self.settings = serde_json::to_vec(settings).map_err(|_| anyhow!("invalid settings"))?;
        ensure!(
            self.settings.len() <= MAX_SETTINGS_BYTES,
            "settings byte limit exceeded"
        );
        Ok(self)
    }
    pub fn origin(&self) -> &str {
        &self.allowed_origin
    }
}

fn validate_url(url: &Url) -> Result<()> {
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "only HTTP(S) sources are supported"
    );
    ensure!(url.host_str().is_some(), "source URL requires a host");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are not supported"
    );
    ensure!(url.fragment().is_none(), "URL fragments are not supported");
    Ok(())
}

pub struct PluginRuntime {
    engine: Engine,
    client: Client,
    limits: Limits,
}

pub struct PreparedPlugin {
    module: Module,
}

#[derive(Debug, Serialize)]
pub struct RunMetrics {
    pub elapsed_ms: f64,
    pub fuel_consumed: u64,
    pub http_requests: u32,
    pub memory_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct RunResult {
    pub dataset: Dataset,
    pub observed_at: Option<u64>,
    pub min_poll_secs: Option<u32>,
    pub additional_datasets: Vec<Dataset>,
    pub additional_observed_at: std::collections::BTreeMap<String, u64>,
    pub metrics: RunMetrics,
}

/// Per-invocation provider hints remain available even when a guest traps.
pub struct RunAttempt {
    pub result: Result<RunResult>,
    pub retry_after_secs: Option<u64>,
    pub provider_backoff: bool,
}
#[derive(Default)]
struct ProviderHints {
    retry_after: AtomicU64,
    backoff: AtomicBool,
}

struct HostState {
    provider_hints: Arc<ProviderHints>,
    limits: StoreLimits,
    budgets: Limits,
    config: RunConfig,
    client: Client,
    requests: u32,
    output: Option<Publication>,
}

impl PluginRuntime {
    pub fn new(limits: Limits) -> Result<Self> {
        ensure!(
            limits.memory_bytes >= 65536,
            "memory budget must allow at least one page"
        );
        ensure!(
            limits.fuel > 0 && limits.max_module_bytes > 0,
            "execution budgets must be positive"
        );
        ensure!(
            limits.max_requests > 0 && !limits.request_timeout.is_zero(),
            "network budgets must be positive"
        );
        for bytes in [limits.max_output_bytes, limits.max_response_bytes] {
            ensure!(
                bytes > 0 && bytes <= i32::MAX as usize,
                "invalid buffer budget"
            );
        }
        let mut config = Config::new();
        config.strategy(Strategy::Cranelift);
        config.consume_fuel(true);
        config.wasm_multi_memory(false);
        config.wasm_memory64(false);
        config.max_wasm_stack(512 * 1024);
        let engine = Engine::new(&config).context("creating native Wasm compiler")?;
        let mut roots = rustls::RootCertStore::empty();
        let native = rustls_native_certs::load_native_certs();
        roots.add_parsable_certificates(native.certs);
        // An explicit provider avoids process-global TLS configuration and
        // reqwest's default AWS-LC dependency. Keep normal certificate checks.
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let client = Client::builder()
            .tls_backend_preconfigured(tls)
            .user_agent(concat!("Paperphase/", env!("CARGO_PKG_VERSION")))
            .redirect(Policy::none())
            .no_proxy()
            .timeout(limits.request_timeout)
            .connect_timeout(limits.request_timeout)
            .pool_max_idle_per_host(2)
            .retry(reqwest::retry::never())
            .build()
            .context("creating shared HTTP client")?;
        Ok(Self {
            engine,
            client,
            limits,
        })
    }

    /// Accept portable binary Wasm only. Never deserialize publisher native code.
    pub fn prepare(&self, wasm: &[u8]) -> Result<PreparedPlugin> {
        ensure!(
            wasm.len() <= self.limits.max_module_bytes,
            "module byte limit exceeded"
        );
        ensure!(
            wasm.starts_with(b"\0asm\x01\0\0\0"),
            "expected a binary WebAssembly module"
        );
        let module = Module::from_binary(&self.engine, wasm).context("compiling plugin")?;
        Ok(PreparedPlugin { module })
    }

    /// Fresh store and quotas per invocation; compiler and HTTP pool are reused.
    pub fn run(&self, plugin: &PreparedPlugin, config: &RunConfig) -> Result<RunResult> {
        self.run_observed(plugin, config).result
    }

    pub fn run_observed(&self, plugin: &PreparedPlugin, config: &RunConfig) -> RunAttempt {
        let hints = Arc::new(ProviderHints::default());
        let result = self.run_inner(plugin, config, Arc::clone(&hints));
        let seconds = hints.retry_after.load(Ordering::Relaxed);
        RunAttempt {
            result,
            retry_after_secs: (seconds > 0).then_some(seconds),
            provider_backoff: hints.backoff.load(Ordering::Relaxed),
        }
    }

    fn run_inner(
        &self,
        plugin: &PreparedPlugin,
        config: &RunConfig,
        hints: Arc<ProviderHints>,
    ) -> Result<RunResult> {
        ensure!(
            Engine::same(&self.engine, plugin.module.engine()),
            "plugin belongs to a different runtime"
        );
        let started = Instant::now();
        let mut store = Store::new(
            &self.engine,
            HostState {
                provider_hints: hints,
                limits: StoreLimitsBuilder::new()
                    .memory_size(self.limits.memory_bytes)
                    .memories(1)
                    .instances(1)
                    .tables(1)
                    .table_elements(4096)
                    .trap_on_grow_failure(true)
                    .build(),
                budgets: self.limits.clone(),
                config: config.clone(),
                client: self.client.clone(),
                requests: 0,
                output: None,
            },
        );
        store.limiter(|state| &mut state.limits);
        // Set before instantiation: a module's start function is untrusted too.
        store.set_fuel(self.limits.fuel)?;
        let mut linker = Linker::new(&self.engine);
        linker.func_wrap(ABI, "source_url", source_url)?;
        linker.func_wrap(ABI, "settings_json", settings_json)?;
        linker.func_wrap(ABI, "http_get", http_get)?;
        linker.func_wrap(ABI, "publish", publish)?;
        linker.func_wrap(ABI, "publish_with_metadata", publish_with_metadata)?;
        let instance = linker
            .instantiate(&mut store, &plugin.module)
            .context("instantiating plugin")?;
        let run = instance
            .get_typed_func::<(), i32>(&mut store, "run")
            .context("plugin requires run() -> i32")?;
        let status = run.call(&mut store, ()).context("executing plugin")?;
        ensure!(status == 0, "plugin returned a failure status");
        let memory_bytes = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow!("plugin must export memory"))?
            .data_size(&store);
        let fuel_consumed = self.limits.fuel - store.get_fuel()?;
        let state = store.data_mut();
        let publication = state
            .output
            .take()
            .ok_or_else(|| anyhow!("plugin did not publish a dataset"))?;
        Ok(RunResult {
            dataset: publication.dataset,
            observed_at: publication.observed_at,
            min_poll_secs: publication.min_poll_secs,
            additional_datasets: publication.additional_datasets,
            additional_observed_at: publication.additional_observed_at,
            metrics: RunMetrics {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                fuel_consumed,
                http_requests: state.requests,
                memory_bytes,
            },
        })
    }
}

fn memory(caller: &mut Caller<'_, HostState>) -> Result<Memory> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| anyhow!("plugin must export memory"))
}

fn buffer(
    caller: &mut Caller<'_, HostState>,
    ptr: i32,
    len: i32,
    limit: usize,
) -> Result<(Memory, usize, usize)> {
    let ptr = usize::try_from(ptr).context("invalid guest buffer pointer")?;
    let len = usize::try_from(len).context("invalid guest buffer length")?;
    ensure!(len <= limit, "guest buffer exceeds host byte limit");
    let memory = memory(caller)?;
    let end = ptr
        .checked_add(len)
        .ok_or_else(|| anyhow!("guest buffer overflow"))?;
    ensure!(
        end <= memory.data_size(&*caller),
        "guest buffer is outside memory"
    );
    Ok((memory, ptr, len))
}

fn read(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32, limit: usize) -> Result<Vec<u8>> {
    let (memory, ptr, len) = buffer(caller, ptr, len, limit)?;
    let mut bytes = vec![0; len];
    memory.read(&*caller, ptr, &mut bytes)?;
    Ok(bytes)
}

fn source_url(mut caller: Caller<'_, HostState>, ptr: i32, capacity: i32) -> Result<i32> {
    let (memory, ptr, capacity) = buffer(&mut caller, ptr, capacity, MAX_URL_BYTES)?;
    let url = caller.data().config.source_url.as_str().as_bytes().to_vec();
    ensure!(url.len() <= capacity, "source URL buffer is too small");
    memory.write(&mut caller, ptr, &url)?;
    Ok(url.len() as i32)
}

fn settings_json(mut caller: Caller<'_, HostState>, ptr: i32, capacity: i32) -> Result<i32> {
    let (memory, ptr, capacity) = buffer(&mut caller, ptr, capacity, MAX_SETTINGS_BYTES)?;
    let bytes = caller.data().config.settings.clone();
    ensure!(bytes.len() <= capacity, "settings buffer is too small");
    memory.write(&mut caller, ptr, &bytes)?;
    Ok(bytes.len() as i32)
}

fn http_get(
    mut caller: Caller<'_, HostState>,
    url_ptr: i32,
    url_len: i32,
    out_ptr: i32,
    capacity: i32,
) -> Result<i32> {
    let max_bytes = caller.data().budgets.max_response_bytes;
    let (memory, out_ptr, capacity) = buffer(&mut caller, out_ptr, capacity, i32::MAX as usize)?;
    ensure!(capacity > 0, "response buffer must not be empty");
    let capacity = capacity.min(max_bytes);
    let bytes = read(&mut caller, url_ptr, url_len, MAX_URL_BYTES)?;
    let url = Url::parse(std::str::from_utf8(&bytes).context("URL must be UTF-8")?)
        .map_err(|_| anyhow!("invalid request URL"))?;
    validate_url(&url)?;
    let state = caller.data_mut();
    ensure!(
        url.origin().ascii_serialization() == state.config.allowed_origin,
        "request origin is not granted"
    );
    ensure!(
        state.requests < state.budgets.max_requests,
        "HTTP request quota exceeded"
    );
    state.requests += 1;
    let response = state
        .client
        .get(url)
        .send()
        .map_err(|_| anyhow!("HTTP request failed or timed out"))?;
    if matches!(response.status().as_u16(), 429 | 503) {
        state.provider_hints.backoff.store(true, Ordering::Relaxed);
        if let Some(value) = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
        {
            let delay = value.parse::<u64>().ok().or_else(|| {
                httpdate::parse_http_date(value).ok().map(|at| {
                    let duration = at.duration_since(SystemTime::now()).unwrap_or_default();
                    duration
                        .as_secs()
                        .saturating_add(u64::from(duration.subsec_nanos() > 0))
                })
            });
            if let Some(delay) = delay {
                state
                    .provider_hints
                    .retry_after
                    .fetch_max(delay, Ordering::Relaxed);
            }
        }
    }
    ensure!(
        response.status().is_success(),
        "HTTP source returned status {}",
        response.status().as_u16()
    );
    // Stream at most capacity + 1, even if Content-Length is absent or false.
    let mut bytes = Vec::new();
    response
        .take(capacity as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow!("HTTP response read failed or timed out"))?;
    ensure!(bytes.len() <= capacity, "HTTP response byte limit exceeded");
    memory.write(&mut caller, out_ptr, &bytes)?;
    Ok(bytes.len() as i32)
}

fn publish(mut caller: Caller<'_, HostState>, ptr: i32, len: i32) -> Result<i32> {
    ensure!(
        caller.data().output.is_none(),
        "plugin may publish only one dataset per invocation"
    );
    let limit = caller.data().budgets.max_output_bytes;
    let bytes = read(&mut caller, ptr, len, limit)?;
    let dataset: Dataset = serde_json::from_slice(&bytes).context("invalid dataset JSON")?;
    dataset.validate()?;
    caller.data_mut().output = Some(Publication {
        dataset,
        observed_at: None,
        min_poll_secs: None,
        additional_datasets: vec![],
        additional_observed_at: Default::default(),
    });
    Ok(0)
}

fn publish_with_metadata(mut caller: Caller<'_, HostState>, ptr: i32, len: i32) -> Result<i32> {
    ensure!(
        caller.data().output.is_none(),
        "plugin may publish only once per invocation"
    );
    let limit = caller.data().budgets.max_output_bytes;
    let bytes = read(&mut caller, ptr, len, limit)?;
    let publication: Publication =
        serde_json::from_slice(&bytes).context("invalid publication JSON")?;
    publication.validate()?;
    caller.data_mut().output = Some(publication);
    Ok(0)
}
