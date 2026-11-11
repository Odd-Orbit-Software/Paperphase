//! Compiled connection execution. A single slot bounds active guest/HTTP memory.
#[cfg(test)]
mod tests;
use anyhow::{Context, Result, ensure};
use axum::{
    Json,
    extract::{Path as RoutePath, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use paperphase_core::{ConnectionConfig, ConnectorCatalog, Snapshot};
use paperphase_runtime::{Limits, PluginRuntime, PreparedPlugin, RunConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::{ApiError, ApiResult, Shared, internal, persist};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegistrationFile {
    schema_version: u32,
    bindings: Vec<Registration>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    connector_id: String,
    wasm: PathBuf,
    #[serde(default)]
    icon_manifest: Option<PathBuf>,
    #[serde(default)]
    request_headers: BTreeMap<String, String>,
    #[serde(default)]
    reference_archive: Option<super::reference::Archive>,
    #[serde(default)]
    work_budget: Option<WorkBudget>,
    source_url: Option<String>,
    #[serde(default)]
    source_setting: Option<String>,
    #[serde(default)]
    origin_interval_secs: Option<u32>,
    stale_after_secs: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkBudget {
    response_bytes: usize,
    fuel: u64,
}

pub(super) struct Binding {
    plugin: PreparedPlugin,
    request_headers: BTreeMap<String, String>,
    references: Option<Arc<super::reference::Source>>,
    work_budget: Option<WorkBudget>,
    config: RunConfig,
    source_setting: Option<String>,
    origin_interval_secs: u64,
    stale_after_secs: u64,
}
pub(super) struct Registry {
    runtime: PluginRuntime,
    pub(super) icons: paperphase_render::IconStore,
    bindings: BTreeMap<String, Binding>,
    floors: BTreeMap<String, u64>,
}
impl Registry {
    pub(super) fn load(path: &Path, catalog: &ConnectorCatalog) -> Result<Self> {
        let file: RegistrationFile = super::read_json(path)?;
        ensure!(
            file.schema_version == 1 && file.bindings.len() <= 32,
            "invalid registration version or binding count"
        );
        let limits = Limits {
            max_requests: 1,
            ..Limits::default()
        };
        // Construct blocking HTTP and compile portable Wasm before entering Tokio.
        let runtime =
            PluginRuntime::new(limits.clone()).map_err(|error| anyhow::anyhow!("{error:#}"))?;
        let mut icons = paperphase_render::IconStore::default();
        let reference_budget = Arc::new(super::reference::Budget::default());
        let mut reference_sources: BTreeMap<String, Arc<super::reference::Source>> =
            BTreeMap::new();
        let mut bindings = BTreeMap::new();
        let mut floors = BTreeMap::new();
        for entry in file.bindings {
            let definition = catalog
                .get(&entry.connector_id)
                .context("registration refers to an unknown connector")?;
            ensure!(
                entry.stale_after_secs > 0 && entry.stale_after_secs <= 604800,
                "staleness interval must be 1–604800 seconds"
            );
            ensure!(
                entry.source_url.is_some() != entry.source_setting.is_some(),
                "registration requires exactly one fixed URL or URL setting"
            );
            if let Some(setting) = &entry.source_setting {
                ensure!(
                    definition.settings.iter().any(|field| &field.id == setting
                        && field.required
                        && matches!(field.input, paperphase_core::SettingInput::Url { .. })),
                    "source setting must be a required URL field"
                );
            }
            ensure!(
                entry.request_headers.len() <= 3,
                "too many authentication headers"
            );
            for (name, setting) in &entry.request_headers {
                ensure!(
                    matches!(
                        name.as_str(),
                        "x-api-key" | "authorization" | "ocp-apim-subscription-key"
                    ) && definition.settings.iter().any(|field| &field.id == setting
                        && field.required
                        && matches!(field.input, paperphase_core::SettingInput::Secret { .. })),
                    "authentication headers must bind supported names to required secret settings"
                );
            }
            let origin_interval = entry
                .origin_interval_secs
                .unwrap_or(definition.min_poll_secs);
            ensure!(
                (1..=86400).contains(&origin_interval),
                "origin interval must be 1–86400 seconds"
            );
            let source = RunConfig::new(
                entry
                    .source_url
                    .as_deref()
                    .unwrap_or("https://example.invalid/"),
            )
            .map_err(|_| anyhow::anyhow!("invalid source URL in connector registration"))?;
            if let Some(budget) = &entry.work_budget {
                source
                    .clone()
                    .with_work_budget(budget.response_bytes, budget.fuel)
                    .map_err(|_| anyhow::anyhow!("invalid connector work budget"))?;
            }
            let references = if let Some(spec) = entry.reference_archive {
                if let Some(setting) = &spec.selection_setting {
                    ensure!(
                        definition.settings.iter().any(|field| &field.id == setting
                            && field.required
                            && matches!(field.input, paperphase_core::SettingInput::Text { .. })),
                        "reference selections require a required text setting"
                    );
                }
                let key = serde_json::to_string(&spec)?;
                if let Some(source) = reference_sources.get(&key) {
                    Some(Arc::clone(source))
                } else {
                    let source = Arc::new(super::reference::Source::with_budget(
                        spec,
                        path.parent().unwrap_or(Path::new(".")),
                        Arc::clone(&reference_budget),
                    )?);
                    reference_sources.insert(key, Arc::clone(&source));
                    Some(source)
                }
            } else {
                None
            };
            if let Some(manifest) = entry.icon_manifest {
                let manifest = path.parent().unwrap_or(Path::new(".")).join(manifest);
                icons.merge(paperphase_render::IconStore::load_pack(
                    &manifest,
                    &entry.connector_id,
                )?)?;
            }
            let module_path = if entry.wasm.is_absolute() {
                entry.wasm
            } else {
                path.parent().unwrap_or(Path::new(".")).join(entry.wasm)
            };
            let mut bytes = Vec::new();
            File::open(module_path)?
                .take(limits.max_module_bytes as u64 + 1)
                .read_to_end(&mut bytes)?;
            let plugin = runtime
                .prepare(&bytes)
                .map_err(|error| anyhow::anyhow!("compiling registered module: {error:#}"))?;
            if entry.source_setting.is_none() {
                floors
                    .entry(source.origin().to_owned())
                    .and_modify(|floor: &mut u64| *floor = (*floor).max(u64::from(origin_interval)))
                    .or_insert(u64::from(origin_interval));
            }
            ensure!(
                bindings
                    .insert(
                        entry.connector_id,
                        Binding {
                            plugin,
                            request_headers: entry.request_headers,
                            references,
                            work_budget: entry.work_budget,
                            config: source,
                            source_setting: entry.source_setting,
                            origin_interval_secs: u64::from(origin_interval),
                            stale_after_secs: entry.stale_after_secs
                        }
                    )
                    .is_none(),
                "duplicate connector registration"
            );
        }
        Ok(Self {
            icons,
            runtime,
            bindings,
            floors,
        })
    }
    pub(super) fn ready(&self, connection: &ConnectionConfig) -> bool {
        self.bindings
            .get(&connection.connector_id)
            .is_some_and(|binding| {
                binding
                    .references
                    .as_ref()
                    .is_none_or(|source| source.ready_for(&connection.settings))
            })
    }
    pub(super) fn supports(&self, id: &str) -> bool {
        self.bindings.contains_key(id)
    }
    pub(super) fn prune_origins(&self, origins: &mut BTreeMap<String, u64>) {
        if !self
            .bindings
            .values()
            .any(|binding| binding.source_setting.is_some())
        {
            origins.retain(|origin, _| self.floors.contains_key(origin));
        }
    }
}

impl Binding {
    fn run_config(&self, settings: &paperphase_core::SettingsValues) -> Result<RunConfig> {
        let config = if let Some(setting) = &self.source_setting {
            RunConfig::new(
                settings
                    .get(setting)
                    .and_then(serde_json::Value::as_str)
                    .context("source URL setting is missing")?,
            )
            .map_err(|_| anyhow::anyhow!("invalid source URL setting"))?
        } else {
            self.config.clone()
        };
        let mut config = config
            .with_settings(settings)
            .map_err(|_| anyhow::anyhow!("invalid connector settings"))?;
        if let Some(budget) = &self.work_budget {
            config = config
                .with_work_budget(budget.response_bytes, budget.fuel)
                .map_err(|_| anyhow::anyhow!("invalid work budget"))?;
        }
        if let Some(data) = self
            .references
            .as_ref()
            .and_then(|source| source.snapshot())
        {
            config = config.with_references(data);
        }
        for (name, setting) in &self.request_headers {
            let value = settings
                .get(setting)
                .and_then(serde_json::Value::as_str)
                .context("required authentication setting is missing")?;
            config = config
                .with_secret_header(name, value)
                .map_err(|_| anyhow::anyhow!("invalid connector authentication setting"))?;
        }
        Ok(config)
    }
}
impl Registry {
    pub(super) fn validate_connection(&self, connection: &ConnectionConfig) -> Result<()> {
        if let Some(binding) = self.bindings.get(&connection.connector_id) {
            binding.run_config(&connection.settings)?;
        }
        Ok(())
    }
    fn floor(&self, origin: &str, connections: &[ConnectionConfig]) -> u64 {
        connections
            .iter()
            .filter_map(|connection| {
                let binding = self.bindings.get(&connection.connector_id)?;
                let config = binding.run_config(&connection.settings).ok()?;
                (config.origin() == origin).then_some(binding.origin_interval_secs)
            })
            .fold(self.floors.get(origin).copied().unwrap_or(1), u64::max)
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ConnectionData {
    pub(super) generation: u64,
    pub(super) snapshot: Option<Snapshot>,
    pub(super) additional_snapshots: Vec<Snapshot>,
    pub(super) last_attempt_at: Option<u64>,
    pub(super) next_fetch_at: u64,
    pub(super) retry_not_before: u64,
    pub(super) failures: u8,
    pub(super) last_error: Option<String>,
    #[serde(skip)]
    pub(super) in_flight: bool,
}

impl ConnectionData {
    pub(super) fn reconfigure(&mut self, generation: u64, now: u64, enabled: bool) {
        self.generation = generation;
        self.in_flight = false;
        self.next_fetch_at = now.max(self.retry_not_before);
        self.last_error = Some(
            if enabled {
                "Settings changed; waiting for updated data."
            } else {
                "Connection disabled."
            }
            .into(),
        );
        for snapshot in self
            .snapshot
            .iter_mut()
            .chain(&mut self.additional_snapshots)
        {
            snapshot.last_error = self.last_error.clone();
        }
    }
}

pub(super) fn eligible_at(
    app: &super::App,
    store: &super::connections::ConnectionStore,
    connection: &ConnectionConfig,
    data: &ConnectionData,
) -> u64 {
    let origin_until = app
        .executors
        .as_ref()
        .and_then(|registry| registry.bindings.get(&connection.connector_id))
        .and_then(|binding| binding.run_config(&connection.settings).ok())
        .and_then(|config| store.origin_deadlines.get(config.origin()).copied())
        .unwrap_or(0);
    data.retry_not_before.max(origin_until)
}

pub(super) fn public_status(
    app: &super::App,
    store: &super::connections::ConnectionStore,
    connection: &ConnectionConfig,
) -> serde_json::Value {
    let empty = ConnectionData::default();
    let data = store.data.get(&connection.id).unwrap_or(&empty);
    let linked = app
        .executors
        .as_ref()
        .is_some_and(|registry| registry.supports(&connection.connector_id));
    let ready = app
        .executors
        .as_ref()
        .is_some_and(|registry| registry.ready(connection));
    let now = (app.clock)();
    let after = eligible_at(app, store, connection, data);
    let status = if linked && store.dirty {
        "storage_error"
    } else if !linked {
        "settings_only"
    } else if !connection.enabled {
        "disabled"
    } else if !ready {
        "preparing"
    } else if data.in_flight {
        "fetching"
    } else if data.failures > 0 {
        "error"
    } else if let Some(snapshot) = &data.snapshot {
        if snapshot.is_stale(now) {
            "stale"
        } else if snapshot.dataset.rows.is_empty() {
            "empty"
        } else {
            "ready"
        }
    } else {
        "waiting"
    };
    json!({"status":status,"can_fetch":linked && ready && !store.dirty && connection.enabled && !data.in_flight && now >= after && app.fetch_slots.available_permits() > 0,
        "fetch_after":after,"next_fetch_at":data.next_fetch_at.max(after),"last_attempt_at":data.last_attempt_at,
        "last_success_at":data.snapshot.as_ref().map(|snapshot| snapshot.last_success_at),"last_error":if store.dirty {Some("Fetching waits for storage recovery.".to_owned())} else {data.last_error.clone()}})
}

pub(super) async fn changed(app: &Shared) {
    let mut workspace = app.workspace.lock().await;
    workspace.desired = None;
    workspace.refresh_wake.notify_one();
    workspace.pending = true;
}
pub(super) fn bump_epoch(epoch: &AtomicU64) {
    epoch.fetch_add(1, Ordering::SeqCst);
}

struct Claim {
    connection: ConnectionConfig,
    run_config: RunConfig,
    origin: String,
    generation: u64,
    failures: u8,
    _permit: OwnedSemaphorePermit,
}
enum Claimed {
    Started(Box<Claim>),
    Wait(u64),
}

async fn flush_metadata(
    app: &Shared,
    store: &mut super::connections::ConnectionStore,
) -> Result<()> {
    if !store.dirty {
        return Ok(());
    }
    let path = app.connections_file.clone();
    let disk = store.clone();
    tokio::task::spawn_blocking(move || persist(&path, &disk)).await??;
    store.dirty = false;
    Ok(())
}

async fn claim(app: &Shared, id: &str) -> ApiResult<Claimed> {
    let registry = app.executors.as_ref().ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "No compiled connector is registered for this service.".into(),
        )
    })?;
    {
        let store = app.connections.lock().await;
        if let Some(connection) = store
            .connections
            .iter()
            .find(|connection| connection.id == id)
            && registry.supports(&connection.connector_id)
            && !registry.ready(connection)
        {
            return Err(ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "Reference data is still preparing. Please wait for the first import.".into(),
            ));
        }
    }
    let permit = Arc::clone(&app.fetch_slots)
        .try_acquire_owned()
        .map_err(|_| {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Another connector is fetching. Try again shortly.".into(),
            )
        })?;
    let mut store = app.connections.lock().await;
    flush_metadata(app, &mut store).await.map_err(internal)?;
    let connection = store
        .connections
        .iter()
        .find(|connection| connection.id == id)
        .cloned()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Connection not found.".into()))?;
    if !connection.enabled {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Enable this connection before fetching.".into(),
        ));
    }
    let binding = registry
        .bindings
        .get(&connection.connector_id)
        .ok_or_else(|| {
            ApiError(
                StatusCode::CONFLICT,
                "This connector has no registered module.".into(),
            )
        })?;
    let run_config = binding
        .run_config(&connection.settings)
        .map_err(invalid_source)?;
    let origin = run_config.origin().to_owned();
    let now = (app.clock)();
    let data = store.data.get(id).cloned().unwrap_or_default();
    let after = eligible_at(app, &store, &connection, &data);
    if now < after {
        return Ok(Claimed::Wait(after.saturating_sub(now)));
    }
    let mut updated = store.clone();
    let state = updated.data.entry(id.to_owned()).or_default();
    state.in_flight = true;
    state.last_attempt_at = Some(now);
    state.next_fetch_at = now.saturating_add(u64::from(connection.poll_interval_secs));
    state.retry_not_before = now.saturating_add(u64::from(
        app.connector_catalog
            .get(&connection.connector_id)
            .expect("validated connector")
            .min_poll_secs,
    ));
    let generation = state.generation;
    let failures = state.failures;
    updated.origin_deadlines.retain(|_, until| *until > now);
    updated.origin_deadlines.insert(
        origin.clone(),
        now.saturating_add(registry.floor(&origin, &store.connections)),
    );
    super::connections::validate_store(&updated, &app.connector_catalog).map_err(internal)?;
    let path = app.connections_file.clone();
    let disk = updated.clone();
    tokio::task::spawn_blocking(move || persist(&path, &disk))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    *store = updated;
    Ok(Claimed::Started(Box::new(Claim {
        connection,
        run_config,
        origin,
        generation,
        failures,
        _permit: permit,
    })))
}

async fn execute(app: Shared, claim: Claim) {
    let registry = Arc::clone(app.executors.as_ref().expect("claim requires registry"));
    let config = claim.connection.clone();
    let run = claim.run_config.clone();
    let result = tokio::task::spawn_blocking(move || {
        let binding = &registry.bindings[&config.connector_id];
        registry.runtime.run_observed(&binding.plugin, &run)
    })
    .await;
    let attempt = result.unwrap_or_else(|_| paperphase_runtime::RunAttempt {
        result: Err(wasmtime_error()),
        retry_after_secs: None,
        provider_backoff: false,
    });
    let mut store = app.connections.lock().await;
    let current = store
        .connections
        .iter()
        .any(|connection| connection.id == claim.connection.id)
        && store
            .data
            .get(&claim.connection.id)
            .is_some_and(|data| data.generation == claim.generation);
    if !current {
        // Editing/deleting a connection invalidates its data, not a provider's
        // response: Retry-After still applies to every user of that origin.
        if attempt.provider_backoff {
            let until = (app.clock)().saturating_add(retry_delay(
                u64::from(claim.connection.poll_interval_secs),
                claim.failures.saturating_add(1),
                attempt.retry_after_secs,
            ));
            store
                .origin_deadlines
                .entry(claim.origin.clone())
                .and_modify(|deadline| *deadline = (*deadline).max(until))
                .or_insert(until);
            store.dirty = true;
            let _ = flush_metadata(&app, &mut store).await;
        }
        return;
    }
    let now = (app.clock)();
    let registry = app.executors.as_ref().expect("registry remains alive");
    let binding = &registry.bindings[&claim.connection.connector_id];
    let mut updated = store.clone();
    let old = updated.data[&claim.connection.id].clone();
    let data = updated
        .data
        .get_mut(&claim.connection.id)
        .expect("claim state");
    data.in_flight = false;
    if let Ok(result) = attempt.result {
        data.snapshot = Some(Snapshot {
            connection_id: claim.connection.id.clone(),
            dataset: result.dataset,
            last_success_at: now,
            observed_at: result.observed_at,
            stale_after_secs: binding.stale_after_secs,
            last_error: None,
        });
        data.additional_snapshots = result
            .additional_datasets
            .into_iter()
            .map(|dataset| Snapshot {
                connection_id: claim.connection.id.clone(),
                observed_at: result.additional_observed_at.get(&dataset.id).copied(),
                dataset,
                last_success_at: now,
                stale_after_secs: binding.stale_after_secs,
                last_error: None,
            })
            .collect();
        if let Some(seconds) = result.min_poll_secs {
            data.retry_not_before = data
                .retry_not_before
                .max(now.saturating_add(u64::from(seconds)));
            data.next_fetch_at = data.next_fetch_at.max(data.retry_not_before);
        }
        data.failures = 0;
        data.last_error = None;
    } else {
        fail(data, &claim.connection, now, attempt.retry_after_secs);
    }
    if super::connections::validate_store(&updated, &app.connector_catalog).is_err()
        || super::connections::merge_snapshots(&updated, &app.snapshots)
            .validate()
            .is_err()
    {
        let data = updated
            .data
            .get_mut(&claim.connection.id)
            .expect("claim state");
        *data = old;
        data.in_flight = false;
        fail(data, &claim.connection, now, None);
        data.last_error = Some("Connector data exceeded the available cache budget.".into());
        for snapshot in data
            .snapshot
            .iter_mut()
            .chain(&mut data.additional_snapshots)
        {
            snapshot.last_error = data.last_error.clone();
        }
    }
    if attempt.provider_backoff {
        let until = updated.data[&claim.connection.id].retry_not_before;
        updated
            .origin_deadlines
            .entry(claim.origin.clone())
            .and_modify(|deadline| *deadline = (*deadline).max(until))
            .or_insert(until);
    }
    let path = app.connections_file.clone();
    let disk = updated.clone();
    let persisted = tokio::task::spawn_blocking(move || persist(&path, &disk)).await;
    if !matches!(persisted, Ok(Ok(()))) {
        // Keep the last durable dataset, but fail closed on further network
        // work until all observed provider deadlines can be written again.
        store.origin_deadlines = updated.origin_deadlines;
        if let Some(data) = store.data.get_mut(&claim.connection.id) {
            data.in_flight = false;
            data.retry_not_before = data
                .retry_not_before
                .max(updated.data[&claim.connection.id].retry_not_before);
            data.next_fetch_at = data.next_fetch_at.max(data.retry_not_before);
            data.failures = data.failures.saturating_add(1);
            data.last_error = Some(
                "Connector cache could not be saved. Fetching waits for storage recovery.".into(),
            );
            for snapshot in data
                .snapshot
                .iter_mut()
                .chain(&mut data.additional_snapshots)
            {
                snapshot.last_error = data.last_error.clone();
            }
        }
        store.dirty = true;
        bump_epoch(&app.data_epoch);
        drop(store);
        changed(&app).await;
        eprintln!("connector completion could not be persisted; keeping the previous cache");
        return;
    }
    *store = updated;
    bump_epoch(&app.data_epoch);
    drop(store);
    changed(&app).await;
}
fn wasmtime_error() -> paperphase_runtime::RuntimeError {
    paperphase_runtime::RuntimeError::msg("connector worker failed")
}

fn fail(
    data: &mut ConnectionData,
    connection: &ConnectionConfig,
    now: u64,
    provider_delay: Option<u64>,
) {
    data.failures = data.failures.saturating_add(1);
    let interval = u64::from(connection.poll_interval_secs);
    let delay = retry_delay(interval, data.failures, provider_delay);
    data.retry_not_before = data.retry_not_before.max(now.saturating_add(delay));
    data.next_fetch_at = data.retry_not_before;
    data.last_error = Some("Connector fetch failed. Last good data is retained.".into());
    for snapshot in data
        .snapshot
        .iter_mut()
        .chain(&mut data.additional_snapshots)
    {
        snapshot.last_error = data.last_error.clone();
    }
}
fn retry_delay(interval: u64, failures: u8, provider_delay: Option<u64>) -> u64 {
    interval
        .saturating_mul(1_u64 << failures.saturating_sub(1).min(6))
        .min(interval.max(3600))
        .max(provider_delay.unwrap_or(0))
}

async fn launch(app: Shared, id: String) -> ApiResult<Option<u64>> {
    match claim(&app, &id).await? {
        Claimed::Wait(seconds) => Ok(Some(seconds)),
        Claimed::Started(claim) => {
            tokio::spawn(execute(app, *claim));
            Ok(None)
        }
    }
}
pub(super) async fn fetch(
    State(app): State<Shared>,
    RoutePath(id): RoutePath<String>,
) -> ApiResult<Response> {
    // Reservation and launch survive HTTP client cancellation as one task.
    let wait = tokio::spawn(launch(app, id)).await.map_err(internal)??;
    Ok(if let Some(seconds) = wait {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, seconds.to_string())],
            Json(json!({"error":"Polling cooldown is active.","retry_after_secs":seconds})),
        )
            .into_response()
    } else {
        (StatusCode::ACCEPTED, Json(json!({"status":"fetching"}))).into_response()
    })
}

pub(super) fn start(app: Shared) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            {
                let mut store = app.connections.lock().await;
                if store.dirty && flush_metadata(&app, &mut store).await.is_err() {
                    continue;
                }
            }
            if app.fetch_slots.available_permits() == 0 {
                continue;
            }
            let mut due = {
                let store = app.connections.lock().await;
                let now = (app.clock)();
                store
                    .connections
                    .iter()
                    .filter(|connection| {
                        connection.enabled
                            && app
                                .executors
                                .as_ref()
                                .is_some_and(|registry| registry.ready(connection))
                    })
                    .filter_map(|connection| {
                        let empty = ConnectionData::default();
                        let data = store.data.get(&connection.id).unwrap_or(&empty);
                        let due = data
                            .next_fetch_at
                            .max(eligible_at(&app, &store, connection, data));
                        // Rank eligible work by its own schedule, before the
                        // shared origin deadline. Otherwise equal deadlines can
                        // let one fast-polling connection starve other stops.
                        (!data.in_flight && now >= due).then_some((
                            data.next_fetch_at.max(data.retry_not_before),
                            connection.id.clone(),
                        ))
                    })
                    .collect::<Vec<_>>()
            };
            due.sort();
            if let Some((_, id)) = due.into_iter().next() {
                let _ = launch(Arc::clone(&app), id).await;
            }
        }
    })
}

pub(super) fn slots() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(1))
}

fn invalid_source(_: anyhow::Error) -> ApiError {
    ApiError(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Invalid connector source settings.".into(),
    )
}

pub(super) fn start_references(app: Shared) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(registry) = &app.executors else {
            return;
        };
        let client = registry.runtime.reference_client();
        let sources = registry
            .bindings
            .iter()
            .filter_map(|(id, binding)| {
                binding
                    .references
                    .as_ref()
                    .map(|source| (id.clone(), Arc::clone(source)))
            })
            .collect::<Vec<_>>();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let connections = {
                let store = app.connections.lock().await;
                store
                    .connections
                    .iter()
                    .filter(|c| c.enabled)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            for (id, source) in &sources {
                let matching = connections
                    .iter()
                    .filter(|c| {
                        registry
                            .bindings
                            .get(&c.connector_id)
                            .and_then(|b| b.references.as_ref())
                            .is_some_and(|other| Arc::ptr_eq(source, other))
                    })
                    .collect::<Vec<_>>();
                if matching.is_empty() {
                    continue;
                }
                let selected = matching
                    .iter()
                    .flat_map(|c| source.selections(&c.settings))
                    .collect();
                source.select(selected);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |at| at.as_secs());
                if source.due(now) {
                    let source = Arc::clone(source);
                    let client = client.clone();
                    let result = tokio::task::spawn_blocking(move || source.update(&client)).await;
                    let reason = match result {
                        Ok(Ok(())) => continue,
                        Ok(Err(error)) => error.to_string(),
                        Err(_) => "reference worker failed".into(),
                    };
                    let reason: String = reason.chars().take(256).collect();
                    eprintln!(
                        "Reference import failed for connector {id}; previous reference data retained: {reason}"
                    );
                }
            }
        }
    })
}
