//! A fixture-backed editor service. Physical refresh policy belongs to a later driver slice.
use std::{
    fs::{File, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
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
use paperphase_render::{ComponentReport, Renderer};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

use crate::simulator::read_json;
mod connections;
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
    render_token: u64,
    refreshing: bool,
    quiet: bool,
    pending: bool,
    last_tick: u64,
    next_due: Option<u64>,
    last_error: Option<String>,
}

struct App {
    connector_catalog: ConnectorCatalog,
    connections: AsyncMutex<connections::ConnectionStore>,
    connections_file: PathBuf,
    workspace: AsyncMutex<Workspace>,
    renderer: Mutex<Renderer>,
    snapshots: SnapshotSet,
    state_file: PathBuf,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
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

fn persist(path: &Path, saved: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .context("workspace requires a parent directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp, saved)?;
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

pub fn run(
    listen: SocketAddr,
    state_file: PathBuf,
    seed_layout: Option<PathBuf>,
    data: Option<PathBuf>,
    connector_definitions: Option<PathBuf>,
    fixed_now: Option<u64>,
) -> Result<()> {
    let state_file = std::path::absolute(state_file)?;
    let parent = state_file.parent().context("invalid workspace path")?;
    std::fs::create_dir_all(parent)?;
    let lock_path = state_file.with_extension("json.lock");
    let connections_file = state_file.with_extension("connections.json");
    ensure!(
        !crate::simulator::same_file(&state_file, &connections_file)?,
        "workspace and connections paths must differ"
    );
    for source in seed_layout
        .iter()
        .chain(data.iter())
        .chain(connector_definitions.iter())
    {
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
    let connector_catalog: ConnectorCatalog = if let Some(path) = connector_definitions {
        read_json(&path)?
    } else {
        serde_json::from_str(include_str!("../../../examples/simulator/connectors.json"))?
    };
    connector_catalog.validate()?;
    let connection_store = if connections_file.try_exists()? {
        read_json(&connections_file)?
    } else {
        connections::ConnectionStore::default()
    };
    connections::validate_store(&connection_store, &connector_catalog)?;
    let snapshots: SnapshotSet = if let Some(path) = data {
        read_json(&path)?
    } else {
        serde_json::from_str(include_str!("../../../examples/simulator/data.json"))?
    };
    snapshots.validate()?;
    let mut renderer = Renderer::new()?;
    let initial_now = now(fixed_now);
    let frame = renderer.render(&saved.layout, &snapshots, initial_now)?;
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
        let app = Arc::new(App {
            connector_catalog,
            connections: AsyncMutex::new(connection_store),
            connections_file,
            workspace: AsyncMutex::new(Workspace::new(saved, png, &snapshots, initial_now)?),
            renderer: Mutex::new(renderer),
            snapshots,
            state_file,
            clock: Arc::new(move || now(fixed_now)),
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
            .route("/api/state", get(state))
            .route("/api/catalog", get(catalog))
            .route("/api/connectors", get(connections::catalog))
            .route("/api/connections", get(connections::list))
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
        let scheduler = scheduling::start(app);
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
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        scheduler.abort();
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
    if !authority_ok || !origin_ok || !site_ok {
        return ApiError(
            StatusCode::FORBIDDEN,
            "Open the editor directly using a host IP, localhost, or the machine hostname.".into(),
        )
        .into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
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
        "frame_id": format!("{}-{}", app.boot_id, ws.frame_serial), "clock_now": (app.clock)(),
        "last_error": ws.last_error,
        "mode": "simulated", "addresses": app.addresses, "fixture_data": true,
    }))
}

async fn display(State(app): State<Shared>) -> Response {
    let png = app.workspace.lock().await.png.clone();
    ([(header::CONTENT_TYPE, "image/png")], Body::from(png)).into_response()
}

async fn catalog(State(app): State<Shared>) -> Json<serde_json::Value> {
    Json(json!(
        app.snapshots
            .snapshots
            .iter()
            .map(|s| json!({
                "connection_id": s.connection_id, "dataset_id": s.dataset.id,
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
}

async fn render(app: Shared, layout: Layout, at: u64) -> ApiResult<Preview> {
    layout.validate().map_err(invalid)?;
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
        let rendered = renderer
            .render(&layout, &app.snapshots, at)
            .map_err(invalid)?;
        let mut png = Vec::new();
        rendered.frame.write_png(&mut png).map_err(internal)?;
        Ok(Preview {
            png_base64: STANDARD.encode(png),
            components: rendered.components,
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
