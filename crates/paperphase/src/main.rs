use std::{
    fs::File,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use paperphase_runtime::{Dataset, Limits, PluginRuntime, RunConfig, RunMetrics};
use serde::Serialize;

mod display;
mod server;
mod simulator;

#[derive(Parser)]
#[command(version, about = "Paperphase e-paper service and development tools")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve a deterministic local temperature API for connector development.
    DemoSource {
        #[arg(long, default_value = "127.0.0.1:9001")]
        listen: std::net::SocketAddr,
    },
    /// Start the LAN editor with a simulated display (no hardware access).
    Serve {
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: std::net::SocketAddr,
        #[arg(long, default_value = ".paperphase/workspace.json")]
        state_file: PathBuf,
        /// Initial layout, used only when no saved workspace exists.
        #[arg(long)]
        seed_layout: Option<PathBuf>,
        /// Optional snapshot fixtures, combined with registered connector results.
        #[arg(long)]
        data: Option<PathBuf>,
        /// Experimental settings descriptors; does not load executable plugins.
        #[arg(long)]
        connector_definitions: Option<PathBuf>,
        /// Local portable-Wasm registrations and explicit HTTP origin grants.
        #[arg(long)]
        connector_bindings: Option<PathBuf>,
        /// Unix seconds; omit to evaluate snapshot freshness using the host clock.
        #[arg(long)]
        now: Option<u64>,
    },
    /// Render a saved layout and snapshots into a monochrome simulated frame.
    Simulate {
        #[arg(long)]
        layout: PathBuf,
        #[arg(long)]
        data: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        pbm: Option<PathBuf>,
        /// Unix seconds; omit to use the host clock for freshness checks.
        #[arg(long)]
        now: Option<u64>,
    },
    /// Run against a deterministic local API; no external service is contacted.
    Demo {
        #[arg(long)]
        plugin: PathBuf,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=1000))]
        iterations: u32,
    },
    /// Run against an explicitly configured HTTP(S) source origin.
    Run {
        #[arg(long)]
        plugin: PathBuf,
        #[arg(long)]
        source_url: String,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=1000))]
        iterations: u32,
    },
}

#[derive(Serialize)]
struct Report {
    architecture: &'static str,
    os: &'static str,
    debug_build: bool,
    runtime_setup_ms: f64,
    compile_ms: f64,
    rss_before_setup_kib: Option<u64>,
    rss_after_compile_kib: Option<u64>,
    peak_rss_kib: Option<u64>,
    dataset: Dataset,
    runs: Vec<RunMetrics>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (plugin, source_url, iterations, _demo) = match args.command {
        Command::DemoSource { listen } => {
            let source = DemoApi::start_at(listen)?;
            println!("Paperphase demo source: {} (fixed 18.5 °C)", source.url);
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async { tokio::signal::ctrl_c().await })?;
            return Ok(());
        }
        Command::Serve {
            listen,
            state_file,
            seed_layout,
            data,
            connector_definitions,
            connector_bindings,
            now,
        } => {
            return server::run(
                listen,
                state_file,
                seed_layout,
                data,
                connector_definitions,
                connector_bindings,
                now,
            );
        }
        Command::Simulate {
            layout,
            data,
            output,
            pbm,
            now,
        } => {
            return simulator::run(&layout, data.as_deref(), &output, pbm.as_deref(), now);
        }
        Command::Demo { plugin, iterations } => {
            let demo = DemoApi::start()?;
            (plugin, demo.url.clone(), iterations, Some(demo))
        }
        Command::Run {
            plugin,
            source_url,
            iterations,
        } => (plugin, source_url, iterations, None),
    };
    let config = RunConfig::new(&source_url).map_err(|error| anyhow::anyhow!("{error:#}"))?;
    let limits = Limits::default();
    let mut wasm = Vec::new();
    File::open(&plugin)
        .context("opening plugin")?
        .take(limits.max_module_bytes as u64 + 1)
        .read_to_end(&mut wasm)?;
    let rss_before_setup_kib = memory_kib("VmRSS:");
    let started = Instant::now();
    let runtime = PluginRuntime::new(limits).map_err(|error| anyhow::anyhow!("{error:#}"))?;
    let runtime_setup_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let plugin = runtime
        .prepare(&wasm)
        .map_err(|error| anyhow::anyhow!("{error:#}"))?;
    let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
    let rss_after_compile_kib = memory_kib("VmRSS:");
    let mut runs = Vec::new();
    let mut dataset = None;
    for _ in 0..iterations {
        let result = runtime
            .run(&plugin, &config)
            .map_err(|error| anyhow::anyhow!("{error:#}"))?;
        dataset = Some(result.dataset);
        runs.push(result.metrics);
    }
    let report = Report {
        architecture: std::env::consts::ARCH,
        os: std::env::consts::OS,
        debug_build: cfg!(debug_assertions),
        runtime_setup_ms,
        compile_ms,
        rss_before_setup_kib,
        rss_after_compile_kib,
        peak_rss_kib: memory_kib("VmHWM:"),
        dataset: dataset.context("no plugin invocation completed")?,
        runs,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn memory_kib(key: &str) -> Option<u64> {
    // Whole-process Linux measurements, including compiler and HTTP machinery.
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix(key))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

struct DemoApi {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DemoApi {
    fn start() -> Result<Self> {
        Self::start_at(([127, 0, 0, 1], 0).into())
    }
    fn start_at(address: std::net::SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(address).context("starting demo API")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}/observations", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
                        let mut request = Vec::new();
                        let mut chunk = [0; 1024];
                        while request.len() < 8192 && !request.windows(4).any(|s| s == b"\r\n\r\n")
                        {
                            match stream.read(&mut chunk) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => request.extend_from_slice(&chunk[..n]),
                            }
                        }
                        let body = r#"{"temperature_c":18.5}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::park_timeout(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            url,
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for DemoApi {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}
