//! Editor, data scheduling and acknowledged simulator/physical display output.
use std::{
    fs::{File, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use paperphase_core::{ConnectorCatalog, Layout, RefreshSettings, SnapshotSet};
use paperphase_render::{ComponentReport, Frame, Renderer};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

use crate::simulator::read_json;
mod agenda;
mod connections;
mod designs;
mod fetching;
mod google;
mod reference;
mod scheduling;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema_version: u32,
    revision: u64,
    layout: Layout,
    #[serde(default)]
    refresh_settings: RefreshSettings,
}

struct Workspace {
    saved: Saved,
    displayed_revision: u64,
    png: Vec<u8>,
    frame_serial: u64,
    completed_frame: Frame,
    completed_layout: Layout,
    controller: crate::display::Controller,
    desired: Option<scheduling::PreparedFrame>,
    worker_running: bool,
    refresh_wake: Arc<tokio::sync::Notify>,
    refreshing: bool,
    quiet: bool,
    pending: bool,
    last_tick: u64,
    next_due: Option<u64>,
    last_error: Option<String>,
    display_frame_available: bool,
    display_warning: Option<String>,
}

struct App {
    driver: Arc<crate::display::driver::Driver>,
    stopping: std::sync::atomic::AtomicBool,
    executors: Option<Arc<fetching::Registry>>,
    google: Option<Arc<google::Service>>,
    fetch_slots: Arc<Semaphore>,
    data_epoch: AtomicU64,
    connector_catalog: ConnectorCatalog,
    connections: AsyncMutex<connections::ConnectionStore>,
    connections_file: PathBuf,
    designs: AsyncMutex<designs::Store>,
    designs_file: PathBuf,
    workspace: AsyncMutex<Workspace>,
    renderer: Mutex<Renderer>,
    snapshots: SnapshotSet,
    state_file: PathBuf,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    clock_fixed: bool,
    boot_id: String,
    tick_lock: AsyncMutex<()>,
    authorities: Vec<String>,
    addresses: Vec<String>,
    render_slots: Arc<Semaphore>,
    // Holding an OS lock also prevents two processes from racing on the same file.
    _file_lock: File,
}

type Shared = Arc<App>;
type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
fn invalid(error: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
}
fn internal(error: impl std::fmt::Display) -> ApiError {
    eprintln!("editor operation failed: {error}");
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "The operation failed. Check the service log; your draft is retained.".into(),
    )
}

pub(crate) fn persist(path: &Path, saved: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(saved)?;
    ensure!(
        bytes.len() < crate::simulator::MAX_JSON_BYTES,
        "saved state exceeds the 1 MiB reload limit"
    );
    let parent = path
        .parent()
        .context("workspace requires a parent directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&bytes)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    // Rename is the commit point: a later directory-sync failure must not make
    // memory disagree with the newly committed file.
    if let Err(error) = File::open(parent).and_then(|file| file.sync_all()) {
        eprintln!("workspace saved, but directory sync failed: {error}");
    }
    Ok(())
}

pub struct Options {
    pub fixed_now: Option<u64>,
    pub display: crate::display::driver::Options,
}

pub fn run(
    listen: SocketAddr,
    state_file: PathBuf,
    seed_layout: Option<PathBuf>,
    data: Option<PathBuf>,
    connector_definitions: Option<PathBuf>,
    connector_bindings: Option<PathBuf>,
    options: Options,
) -> Result<()> {
    let fixed_now = options.fixed_now;
    ensure!(
        fixed_now.is_none()
            || matches!(
                options.display.kind,
                crate::display::driver::Kind::Simulator
            ),
        "physical output requires the real host clock"
    );
    let state_file = std::path::absolute(state_file)?;
    let parent = state_file.parent().context("invalid workspace path")?;
    std::fs::create_dir_all(parent)?;
    let lock_path = state_file.with_extension("json.lock");
    let connections_file = state_file.with_extension("connections.json");
    let google_file = state_file.with_extension("google.json");
    let designs_file = state_file.with_extension("designs.json");
    let display_file = state_file.with_extension("display.json");
    for other in [
        &state_file,
        &connections_file,
        &google_file,
        &designs_file,
        &lock_path,
    ] {
        ensure!(
            !crate::simulator::same_file(&display_file, other)?,
            "display journal must be separate from other state"
        );
    }
    for other in [&state_file, &connections_file, &google_file, &lock_path] {
        ensure!(
            !crate::simulator::same_file(&designs_file, other)?,
            "design storage must be separate from other state"
        );
    }
    let design_store = designs::load(&designs_file)?;
    ensure!(
        !crate::simulator::same_file(&state_file, &connections_file)?,
        "workspace and connections paths must differ"
    );
    for other in [&state_file, &connections_file, &lock_path] {
        ensure!(
            !crate::simulator::same_file(&google_file, other)?,
            "Google account storage must be separate from workspace, connections and lock files"
        );
    }
    for source in seed_layout
        .iter()
        .chain(data.iter())
        .chain(connector_definitions.iter())
        .chain(connector_bindings.iter())
    {
        ensure!(
            !crate::simulator::same_file(&display_file, source)?,
            "display journal must not replace an input"
        );
        ensure!(
            !crate::simulator::same_file(&designs_file, source)?,
            "design storage must not replace an input"
        );
        ensure!(
            !crate::simulator::same_file(&state_file, source)?,
            "workspace must not replace a fixture input"
        );
        ensure!(
            !crate::simulator::same_file(&lock_path, source)?,
            "workspace lock must not replace a fixture input"
        );
        ensure!(
            !crate::simulator::same_file(&connections_file, source)?,
            "connection storage must not replace an input"
        );
        ensure!(
            !crate::simulator::same_file(&google_file, source)?,
            "Google account storage must not replace an input"
        );
    }
    let file_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    file_lock
        .try_lock()
        .context("another service owns this workspace")?;
    let exists = state_file.try_exists()?;
    let saved: Saved = if exists {
        read_json(&state_file)?
    } else {
        let layout = if let Some(path) = seed_layout {
            read_json(&path)?
        } else {
            let mut layout: Layout =
                serde_json::from_str(include_str!("../../../examples/simulator/layout.json"))?;
            layout.components.truncate(3);
            layout
        };
        Saved {
            schema_version: 1,
            revision: 0,
            layout,
            refresh_settings: RefreshSettings::default(),
        }
    };
    ensure!(
        saved.schema_version == 1,
        "unsupported workspace schema version"
    );
    saved.layout.validate()?;
    saved.refresh_settings.validate()?;
    let driver = Arc::new(crate::display::driver::Driver::open(
        options.display,
        &saved.layout,
        display_file,
    )?);
    let connector_catalog: ConnectorCatalog = if let Some(path) = connector_definitions {
        read_json(&path)?
    } else {
        serde_json::from_str(include_str!("../../../examples/simulator/connectors.json"))?
    };
    connector_catalog.validate()?;
    let mut connection_store = if connections_file.try_exists()? {
        read_json(&connections_file)?
    } else {
        connections::ConnectionStore::default()
    };
    connections::validate_store(&connection_store, &connector_catalog)?;
    let executors = connector_bindings
        .as_deref()
        .map(|path| fetching::Registry::load(path, &connector_catalog).map(Arc::new))
        .transpose()?;
    let client = if let Some(registry) = &executors {
        registry.http_client()
    } else {
        paperphase_runtime::shared_http_client(std::time::Duration::from_secs(3))
            .map_err(|error| anyhow::anyhow!("{error:#}"))?
    };
    let google = Some(Arc::new(google::Service::open(google_file, client)?));
    if let Some(registry) = &executors {
        registry.prune_origins(&mut connection_store.origin_deadlines);
        for connection in &connection_store.connections {
            registry.validate_connection(connection)?;
        }
    }
    let snapshots: SnapshotSet = if let Some(path) = data {
        read_json(&path)?
    } else {
        serde_json::from_str(include_str!("../../../examples/simulator/data.json"))?
    };
    snapshots.validate()?;
    ensure!(
        snapshots.snapshots.len() + connection_store.connections.len() <= 256,
        "snapshot source limit exceeded"
    );
    ensure!(
        !connection_store
            .connections
            .iter()
            .any(|connection| snapshots
                .snapshots
                .iter()
                .any(|snapshot| snapshot.connection_id == connection.id)),
        "connection id conflicts with a fixture source"
    );
    let initial_data = connections::merge_snapshots(&connection_store, &snapshots);
    initial_data.validate()?;
    let mut renderer = Renderer::with_icons(
        executors
            .as_ref()
            .map(|registry| registry.icons.clone())
            .unwrap_or_default(),
    )?;
    let initial_now = now(fixed_now);
    let frame = renderer.render(&saved.layout, &initial_data, initial_now)?;
    let mut png = Vec::new();
    frame.frame.write_png(&mut png)?;
    if !exists {
        persist(&state_file, &saved)?;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(listen)
            .await
            .context("binding editor service")?;
        let address = listener.local_addr()?;
        let mut hosts = vec!["localhost".to_owned(), "127.0.0.1".into(), "[::1]".into()];
        let mut addresses = Vec::new();
        for interface in if_addrs::get_if_addrs()? {
            let ip = interface.ip();
            let host = match ip {
                std::net::IpAddr::V4(_) => ip.to_string(),
                std::net::IpAddr::V6(_) => format!("[{ip}]"),
            };
            if !interface.is_loopback()
                && !matches!(ip, std::net::IpAddr::V6(v) if v.is_unicast_link_local())
            {
                let priority = if ["en", "eth", "wl"]
                    .iter()
                    .any(|prefix| interface.name.starts_with(prefix))
                {
                    0
                } else {
                    1
                };
                addresses.push((priority, SocketAddr::new(ip, address.port()).to_string()));
            }
            hosts.push(host);
        }
        if let Ok(hostname) = std::fs::read_to_string("/etc/hostname") {
            let hostname = hostname.trim().to_ascii_lowercase();
            if !hostname.is_empty()
                && hostname
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            {
                hosts.extend([format!("{hostname}.local"), hostname]);
            }
        }
        hosts.sort();
        hosts.dedup();
        addresses.sort();
        addresses.dedup();
        let addresses = addresses.into_iter().map(|(_, address)| address).collect();
        let authorities = hosts
            .into_iter()
            .flat_map(|host| {
                let explicit = format!("{host}:{}", address.port());
                if address.port() == 80 {
                    vec![host, explicit]
                } else {
                    vec![explicit]
                }
            })
            .collect();
        let mut workspace = Workspace::new(saved, png, frame.frame, &initial_data, initial_now)?;
        if driver.is_physical() {
            workspace.controller.profile = driver.profile();
            workspace.controller.baseline_known = false;
            workspace.controller.retry_at =
                Some(tokio::time::Instant::now() + driver.restart_wait());
            workspace.pending = true;
            if let Some(completed) = driver.restore()? {
                workspace.displayed_revision = completed.revision;
                workspace.completed_layout = completed.layout;
                workspace.completed_frame = completed.frame;
                workspace.png.clear();
                workspace.completed_frame.write_png(&mut workspace.png)?;
            } else {
                workspace.display_frame_available = false;
                workspace.completed_frame = Frame::blank(&workspace.saved.layout.display)?;
                workspace.completed_layout = Layout {
                    schema_version: 1,
                    display: workspace.saved.layout.display.clone(),
                    components: vec![],
                };
                workspace.png.clear();
                workspace.completed_frame.write_png(&mut workspace.png)?;
            }
        }
        let app = Arc::new(App {
            driver,
            stopping: std::sync::atomic::AtomicBool::new(false),
            executors,
            google,
            fetch_slots: fetching::slots(),
            data_epoch: AtomicU64::new(0),
            connector_catalog,
            connections: AsyncMutex::new(connection_store),
            connections_file,
            designs: AsyncMutex::new(design_store),
            designs_file,
            workspace: AsyncMutex::new(workspace),
            renderer: Mutex::new(renderer),
            snapshots,
            state_file,
            clock: Arc::new(move || now(fixed_now)),
            clock_fixed: fixed_now.is_some(),
            boot_id: format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ),
            tick_lock: AsyncMutex::new(()),
            authorities,
            addresses,
            render_slots: Arc::new(Semaphore::new(4)),
            _file_lock: file_lock,
        });
        let routes = Router::new()
            .route(
                "/",
                get(|| async {
                    asset(
                        "text/html; charset=utf-8",
                        include_bytes!("../../../web/dist/index.html"),
                    )
                }),
            )
            .route(
                "/app.js",
                get(|| async {
                    asset(
                        "text/javascript; charset=utf-8",
                        include_bytes!("../../../web/dist/app.js"),
                    )
                }),
            )
            .route(
                "/app.css",
                get(|| async {
                    asset(
                        "text/css; charset=utf-8",
                        include_bytes!("../../../web/dist/app.css"),
                    )
                }),
            )
            .route(
                "/brand/odd-orbit.png",
                get(|| async {
                    asset(
                        "image/png",
                        include_bytes!("../../../web/assets/brand/odd-orbit.png"),
                    )
                }),
            )
            .route(
                "/brand/orbi.png",
                get(|| async {
                    asset(
                        "image/png",
                        include_bytes!("../../../web/assets/brand/orbi.png"),
                    )
                }),
            )
            .route("/api/designs", get(designs::list))
            .route(
                "/api/designs/{id}",
                put(designs::save).delete(designs::remove),
            )
            .route("/api/component-fit", post(designs::fit))
            .route("/api/state", get(state))
            .route("/api/catalog", get(catalog))
            .route(
                "/api/icon-packs",
                get(|State(app): State<Shared>| async move {
                    Json(
                        app.executors
                            .as_ref()
                            .map(|registry| registry.icons.catalog())
                            .unwrap_or_default(),
                    )
                }),
            )
            .route("/api/connectors", get(connections::catalog))
            .route("/api/connections", get(connections::list))
            .route("/api/google", get(google::status))
            .route("/api/google/config", put(google::configure))
            .route("/api/google/begin", post(google::begin))
            .route(
                "/api/google/accounts/{id}",
                axum::routing::delete(google::remove),
            )
            .route(google::CALLBACK, get(google::callback))
            .route("/api/connections/{id}/fetch", post(fetching::fetch))
            .route(
                "/api/connections/{id}",
                put(connections::save).delete(connections::remove),
            )
            .route(
                "/api/timezones",
                get(|| async { Json(paperphase_core::timezones()) }),
            )
            .route("/api/display.png", get(display))
            .route("/api/preview", post(preview))
            .route("/api/layout", put(save))
            .route("/api/refresh", post(refresh))
            .layer(DefaultBodyLimit::max(1024 * 1024))
            .layer(middleware::from_fn_with_state(
                Arc::clone(&app),
                same_origin,
            ))
            .with_state(Arc::clone(&app));
        let reference_scheduler = fetching::start_references(Arc::clone(&app));
        let fetch_scheduler = fetching::start(Arc::clone(&app));
        let scheduler = scheduling::start(Arc::clone(&app));
        let browser_address = if address.ip().is_unspecified() {
            SocketAddr::new(
                if address.is_ipv4() {
                    std::net::Ipv4Addr::LOCALHOST.into()
                } else {
                    std::net::Ipv6Addr::LOCALHOST.into()
                },
                address.port(),
            )
        } else {
            address
        };
        println!("Paperphase editor: http://{browser_address}");
        std::io::stdout().flush()?;
        axum::serve(listener, routes)
            .with_graceful_shutdown({
                let app = Arc::clone(&app);
                async move {
                    #[cfg(unix)]
                    {
                        let mut term = tokio::signal::unix::signal(
                            tokio::signal::unix::SignalKind::terminate(),
                        )
                        .expect("install SIGTERM handler");
                        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = tokio::signal::ctrl_c().await;
                    }
                    app.stopping.store(true, Ordering::SeqCst);
                    app.workspace.lock().await.refresh_wake.notify_one();
                }
            })
            .await?;
        scheduler.abort();
        reference_scheduler.abort();
        fetch_scheduler.abort();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(95);
        while app.workspace.lock().await.worker_running && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Ok(())
    })
}

fn now(fixed: Option<u64>) -> u64 {
    fixed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    })
}

fn asset(content_type: &'static str, bytes: &'static [u8]) -> Response {
    ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
}

async fn same_origin(State(app): State<Shared>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let authority_ok = app.authorities.contains(&host);
    let origin_ok = request.headers().get(header::ORIGIN).is_none_or(|h| {
        h.to_str()
            .is_ok_and(|origin| origin == format!("http://{host}"))
    });
    let site_ok = request
        .headers()
        .get("sec-fetch-site")
        .is_none_or(|h| h == "same-origin" || h == "none");
    // The hosted relay returns a cross-site GET; the callback verifies its own
    // one-use pending state before changing account links.
    let oauth_return =
        request.method() == axum::http::Method::GET && request.uri().path() == google::CALLBACK;
    // Fetch metadata retains "cross-site" through the callback's redirect to
    // the editor. The public HTML shell is safe to navigate to; its API calls
    // still require the ordinary same-origin checks.
    let editor_navigation = request.method() == axum::http::Method::GET
        && request.uri().path() == "/"
        && request
            .headers()
            .get("sec-fetch-mode")
            .is_some_and(|h| h == "navigate")
        && request
            .headers()
            .get("sec-fetch-dest")
            .is_some_and(|h| h == "document");
    let request_allowed = oauth_return || editor_navigation || (origin_ok && site_ok);
    if !authority_ok || !request_allowed {
        return ApiError(
            StatusCode::FORBIDDEN,
            "Open the editor directly using a host IP, localhost, or the machine hostname.".into(),
        )
        .into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("content-security-policy", HeaderValue::from_static("default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"));
    response
}

async fn state(State(app): State<Shared>) -> Json<serde_json::Value> {
    let ws = app.workspace.lock().await;
    Json(json!({
        "revision": ws.saved.revision, "layout": ws.saved.layout, "refresh_settings": ws.saved.refresh_settings,
        "displayed_revision": ws.displayed_revision,
        "phase": ws.phase(), "pending": ws.pending, "next_update_at": ws.next_due,
        "frame_id": format!("{}-{}", app.boot_id, ws.frame_serial), "clock_now": (app.clock)(), "clock_fixed": app.clock_fixed,
        "last_error": ws.last_error,
        "refresh": {"profile": ws.controller.profile, "active": ws.controller.active,
            "last_completed": ws.controller.completed, "partial_count": ws.controller.partials,
            "baseline_known": ws.controller.baseline_known, "failures": ws.controller.failures,
            "wait_ms": ws.controller.ready_at(&ws.saved.refresh_settings.policy, tokio::time::Instant::now()).saturating_duration_since(tokio::time::Instant::now()).as_millis()},
        "data_revision": app.data_epoch.load(Ordering::SeqCst),
        "mode": if app.driver.is_physical(){"physical"}else{"simulated"}, "display_frame_available":ws.display_frame_available,"display_warning":ws.display_warning, "addresses": app.addresses, "fixture_data": !app.snapshots.snapshots.is_empty(), "connector_execution": app.executors.is_some(),
    }))
}

async fn display(State(app): State<Shared>) -> Response {
    let png = app.workspace.lock().await.png.clone();
    ([(header::CONTENT_TYPE, "image/png")], Body::from(png)).into_response()
}

async fn catalog(State(app): State<Shared>) -> Json<serde_json::Value> {
    let store = app.connections.lock().await;
    let snapshots = connections::merge_snapshots(&store, &app.snapshots);
    Json(json!(
        snapshots
            .snapshots
            .iter()
            .map(|s| json!({
                "connection_id": s.connection_id, "dataset_id": s.dataset.id,
                "connection_label": store.connections.iter().find(|connection| connection.id == s.connection_id).map_or(s.connection_id.as_str(), |connection| connection.label.as_str()),
                "fields": s.dataset.fields, "rows": s.dataset.rows.len(),
                "stale": s.is_stale((app.clock)()),
            }))
            .collect::<Vec<_>>()
    ))
}

#[derive(Serialize)]
struct Preview {
    png_base64: String,
    components: Vec<ComponentReport>,
    #[serde(skip)]
    data_epoch: u64,
    #[serde(skip)]
    frame: Frame,
    #[serde(skip)]
    next_due: Option<u64>,
}

async fn render(app: Shared, layout: Layout, at: u64) -> ApiResult<Preview> {
    layout.validate().map_err(invalid)?;
    let (data, data_epoch) = {
        let store = app.connections.lock().await;
        (
            connections::merge_snapshots(&store, &app.snapshots),
            app.data_epoch.load(Ordering::SeqCst),
        )
    };
    let next_due = scheduling::next_due(&layout, &data, at).map_err(invalid)?;
    let permit = Arc::clone(&app.render_slots)
        .try_acquire_owned()
        .map_err(|_| {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Renderer busy. Try again shortly.".into(),
            )
        })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut renderer = app.renderer.lock().map_err(internal)?;
        let rendered = renderer.render(&layout, &data, at).map_err(invalid)?;
        let mut png = Vec::new();
        rendered.frame.write_png(&mut png).map_err(internal)?;
        Ok(Preview {
            png_base64: STANDARD.encode(png),
            components: rendered.components,
            frame: rendered.frame,
            data_epoch,
            next_due,
        })
    })
    .await
    .map_err(internal)?
}

async fn preview(
    State(app): State<Shared>,
    Json(layout): Json<Layout>,
) -> ApiResult<Json<Preview>> {
    let at = (app.clock)();
    render(app, layout, at).await.map(Json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    base_revision: u64,
    layout: Layout,
    #[serde(default)]
    refresh_settings: Option<RefreshSettings>,
}

async fn save(
    State(app): State<Shared>,
    Json(request): Json<SaveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let ws = app.workspace.lock().await;
    if request.base_revision != ws.saved.revision {
        return Err(ApiError(StatusCode::CONFLICT, "A newer layout was saved in another tab. Your draft is retained; discard it to load that layout.".into()));
    }
    if request.layout.display.id != ws.saved.layout.display.id
        || request.layout.display.width != ws.saved.layout.display.width
        || request.layout.display.height != ws.saved.layout.display.height
    {
        return Err(invalid(
            "Display profile changes require a service restart with a new workspace.",
        ));
    }
    let refresh_settings = request
        .refresh_settings
        .unwrap_or_else(|| ws.saved.refresh_settings.clone());
    refresh_settings.validate().map_err(invalid)?;
    let prepared = scheduling::prepare(
        Arc::clone(&app),
        &request.layout,
        &refresh_settings,
        (app.clock)(),
    )
    .await?;
    let saved = Saved {
        schema_version: 1,
        revision: ws
            .saved
            .revision
            .checked_add(1)
            .ok_or_else(|| internal("revision exhausted"))?,
        layout: request.layout,
        refresh_settings,
    };
    let path = app.state_file.clone();
    let to_write = saved.clone();
    // Keep ownership of the transaction in a spawned task: disconnecting the
    // browser must not cancel persistence between disk commit and memory commit.
    let app_commit = Arc::clone(&app);
    drop(ws);
    tokio::spawn(async move {
        let mut ws = app_commit.workspace.lock().await;
        if ws.saved.revision != saved.revision - 1 {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "A newer layout was saved. Your draft is retained.".into(),
            ));
        }
        tokio::task::spawn_blocking(move || persist(&path, &to_write))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        let revision = saved.revision;
        ws.saved = saved;
        scheduling::queue(&app_commit, &mut ws, prepared);
        Ok(Json(json!({"revision": revision, "phase": ws.phase()})))
    })
    .await
    .map_err(internal)?
}

async fn refresh(State(app): State<Shared>) -> ApiResult<Json<serde_json::Value>> {
    scheduling::tick(Arc::clone(&app), true).await?;
    let ws = app.workspace.lock().await;
    Ok(Json(json!({"phase": ws.phase(), "pending": ws.pending})))
}
