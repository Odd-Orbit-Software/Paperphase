use super::*;
use crate::server::{connections, scheduling, scheduling::tests as support};
use axum::extract::{Path as RoutePath, State};
use paperphase_core::{Layout, RefreshSettings};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize},
    },
    thread,
    time::{Duration, Instant},
};

const START: u64 = 1_699_999_980;
fn dataset(value: f64) -> Value {
    json!({"schema_version":1,"id":"current-weather","fields":[{"id":"temperature","kind":"number"}],"rows":[{"temperature":value}]})
}
#[derive(Clone)]
struct Reply {
    status: u16,
    retry: Option<u64>,
    body: Value,
    raw: Option<Vec<u8>>,
    delay: Duration,
}
struct Source {
    url: String,
    reply: Arc<Mutex<Reply>>,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Source {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/observation", listener.local_addr().unwrap());
        let reply = Arc::new(Mutex::new(Reply {
            status: 200,
            retry: None,
            body: dataset(18.5),
            raw: None,
            delay: Duration::ZERO,
        }));
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let response = Arc::clone(&reply);
        let requests = Arc::clone(&hits);
        let done = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                        let mut buffer = [0; 8192];
                        let _ = stream.read(&mut buffer);
                        let reply = response.lock().unwrap().clone();
                        requests.fetch_add(1, Ordering::SeqCst);
                        if !reply.delay.is_zero() {
                            thread::sleep(reply.delay);
                        }
                        let body = reply
                            .raw
                            .unwrap_or_else(|| serde_json::to_vec(&reply.body).unwrap());
                        let retry = reply.retry.map_or(String::new(), |seconds| {
                            format!("Retry-After: {seconds}\r\n")
                        });
                        let header = format!(
                            "HTTP/1.1 {} Result\r\n{retry}Content-Length: {}\r\nConnection: close\r\n\r\n",
                            reply.status,
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url,
            reply,
            hits,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.worker.take().unwrap().join();
    }
}

fn setup(source: &Source, quiet: bool) -> (Shared, Arc<AtomicU64>, tempfile::TempDir) {
    let mut scene = serde_json::to_value(support::scene()).unwrap();
    scene["components"][0]["blocks"] = json!([{"id":"value","font_size":32,"source":{"kind":"field","binding":{"connection_id":"home","dataset_id":"current-weather","field_id":"temperature"}}}]);
    let layout: Layout = serde_json::from_value(scene).unwrap();
    let (mut app, clock, dir) = support::app(
        layout,
        if quiet {
            support::quiet()
        } else {
            RefreshSettings::default()
        },
        support::empty(),
        START,
    );
    let runtime = PluginRuntime::new(Limits {
        max_requests: 1,
        ..Limits::default()
    })
    .unwrap();
    let wasm = wat::parse_str(r#"(module
        (import "paperphase_v0" "source_url" (func $source (param i32 i32) (result i32)))
        (import "paperphase_v0" "http_get" (func $get (param i32 i32 i32 i32) (result i32)))
        (import "paperphase_v0" "publish" (func $publish (param i32 i32) (result i32)))
        (memory (export "memory") 2)
        (func (export "run") (result i32) (local $length i32)
          i32.const 0 i32.const 8192 call $source local.set $length
          i32.const 16384 i32.const 0 local.get $length i32.const 16384 i32.const 65536 call $get call $publish drop
          i32.const 0))"#).unwrap();
    let config = RunConfig::new(&source.url).unwrap();
    let origin = config.origin().to_owned();
    let plugin = runtime.prepare(&wasm).unwrap();
    let registry = Registry {
        icons: paperphase_render::IconStore::default(),
        runtime,
        bindings: BTreeMap::from([(
            "example.temperature".into(),
            Binding {
                plugin,
                config,
                source_setting: None,
                origin_interval_secs: 5,
                stale_after_secs: 60,
            },
        )]),
        floors: BTreeMap::from([(origin, 5)]),
    };
    let mutable = Arc::get_mut(&mut app).unwrap();
    let definition = mutable
        .connector_catalog
        .connectors
        .iter_mut()
        .find(|definition| definition.id == "example.temperature")
        .unwrap();
    definition.min_poll_secs = 5;
    definition.default_poll_secs = 5;
    mutable.executors = Some(Arc::new(registry));
    (app, clock, dir)
}
async fn configure(app: &Shared, id: &str) {
    let revision = app.connections.lock().await.revision;
    let request = serde_json::from_value(json!({"base_revision":revision,"connector_id":"example.temperature","label":"Test connection","enabled":true,"poll_interval_secs":5,"settings":{}})).unwrap();
    let _ = connections::save(State(Arc::clone(app)), RoutePath(id.into()), Json(request))
        .await
        .unwrap();
}
async fn complete(app: &Shared) {
    for _ in 0..600 {
        if app.fetch_slots.available_permits() == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("fetch did not finish");
}
async fn hit(source: &Source, count: usize) {
    let until = Instant::now() + Duration::from_secs(3);
    while source.hits.load(Ordering::SeqCst) < count {
        assert!(Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
fn run(body: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(body);
}

#[test]
fn fetched_snapshots_are_durable_account_scoped_and_update_pixels_without_layout_revisions() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, false);
    run(async {
        configure(&app, "home").await;
        configure(&app, "work").await;
        assert!(
            launch(Arc::clone(&app), "home".into())
                .await
                .unwrap()
                .is_none()
        );
        complete(&app).await;
        scheduling::tick(Arc::clone(&app), false).await.unwrap();
        support::settle(&app).await;
        support::assert_current_pixels(&app).await;
        assert_eq!(app.workspace.lock().await.saved.revision, 0);
        assert_eq!(
            launch(Arc::clone(&app), "work".into()).await.unwrap(),
            Some(5)
        );
        assert_eq!(source.hits.load(Ordering::SeqCst), 1);
        clock.store(START + 5, Ordering::SeqCst);
        source.reply.lock().unwrap().body = dataset(27.0);
        launch(Arc::clone(&app), "work".into()).await.unwrap();
        complete(&app).await;
        let store = app.connections.lock().await;
        assert_eq!(store.revision, 2);
        assert_eq!(
            store.data["home"].snapshot.as_ref().unwrap().dataset.rows[0]["temperature"],
            18.5
        );
        assert_eq!(
            store.data["work"].snapshot.as_ref().unwrap().dataset.rows[0]["temperature"],
            27.0
        );
        let disk: connections::ConnectionStore =
            super::super::read_json(&app.connections_file).unwrap();
        assert_eq!(
            disk.data["work"].snapshot.as_ref().unwrap().last_success_at,
            START + 5
        );
    });
}

#[test]
fn an_older_prepared_frame_cannot_publish_after_another_fetch_changes_its_data() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, false);
    run(async {
        configure(&app, "home").await;
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        complete(&app).await;
        let saved = app.workspace.lock().await.saved.clone();
        let obsolete = scheduling::prepare(
            Arc::clone(&app),
            &saved.layout,
            &saved.refresh_settings,
            (app.clock)(),
        )
        .await
        .unwrap();
        source.reply.lock().unwrap().body = dataset(27.0);
        clock.store(START + 5, Ordering::SeqCst);
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        complete(&app).await;
        let mut workspace = app.workspace.lock().await;
        scheduling::queue(&app, &mut workspace, obsolete);
        assert_eq!(workspace.phase(), "pending");
        assert!(workspace.pending);
        drop(workspace);
        scheduling::tick(Arc::clone(&app), false).await.unwrap();
        support::settle(&app).await;
        support::assert_current_pixels(&app).await;
    });
}

#[test]
fn failure_keeps_last_good_data_and_persistent_provider_backoff_applies_across_connections() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, false);
    run(async {
        configure(&app, "home").await;
        configure(&app, "work").await;
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        complete(&app).await;
        clock.store(START + 5, Ordering::SeqCst);
        {
            let mut reply = source.reply.lock().unwrap();
            reply.status = 429;
            reply.retry = Some(40);
        }
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        complete(&app).await;
        let mut store = app.connections.lock().await;
        assert_eq!(store.data["home"].failures, 1);
        let snapshot = store.data["home"].snapshot.as_ref().unwrap();
        assert_eq!(snapshot.dataset.rows[0]["temperature"], 18.5);
        assert!(snapshot.is_stale(START + 5));
        // Reload durable state with in-flight flags cleared, as at service restart.
        *store = super::super::read_json(&app.connections_file).unwrap();
        drop(store);
        configure(&app, "home").await;
        assert_eq!(
            launch(Arc::clone(&app), "home".into()).await.unwrap(),
            Some(40)
        );
        assert_eq!(
            launch(Arc::clone(&app), "work".into()).await.unwrap(),
            Some(40)
        );
        assert_eq!(source.hits.load(Ordering::SeqCst), 2);
    });
}

#[test]
fn quiet_hours_allow_fetching_but_resume_only_the_latest_pixels() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, true);
    run(async {
        configure(&app, "home").await;
        let original = app.workspace.lock().await.png.clone();
        clock.store(START + 60, Ordering::SeqCst);
        scheduling::tick(Arc::clone(&app), false).await.unwrap();
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        complete(&app).await;
        clock.store(START + 120, Ordering::SeqCst);
        source.reply.lock().unwrap().body = dataset(29.0);
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        complete(&app).await;
        scheduling::tick(Arc::clone(&app), false).await.unwrap();
        assert_eq!(app.workspace.lock().await.png, original);
        clock.store(START + 180, Ordering::SeqCst);
        scheduling::tick(Arc::clone(&app), false).await.unwrap();
        support::settle(&app).await;
        support::assert_current_pixels(&app).await;
        let ws = app.workspace.lock().await;
        assert_eq!(ws.frame_serial, 1);
        assert_eq!(ws.saved.revision, 0);
    });
}

#[test]
fn edited_or_recreated_connections_discard_late_data_without_dropping_provider_hints() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, false);
    run(async {
        configure(&app, "home").await;
        {
            let mut reply = source.reply.lock().unwrap();
            reply.status = 429;
            reply.retry = Some(90);
            reply.delay = Duration::from_millis(150);
        }
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        hit(&source, 1).await;
        configure(&app, "home").await;
        complete(&app).await;
        assert!(app.connections.lock().await.data["home"].snapshot.is_none());
        assert_eq!(
            launch(Arc::clone(&app), "home".into()).await.unwrap(),
            Some(90)
        );
        clock.store(START + 90, Ordering::SeqCst);
        {
            let mut reply = source.reply.lock().unwrap();
            reply.status = 200;
            reply.retry = None;
        }
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        hit(&source, 2).await;
        let revision = app.connections.lock().await.revision;
        let _ = connections::remove(
            State(Arc::clone(&app)),
            RoutePath("home".into()),
            Json(serde_json::from_value(json!({"base_revision":revision})).unwrap()),
        )
        .await
        .unwrap();
        configure(&app, "home").await;
        complete(&app).await;
        assert!(app.connections.lock().await.data["home"].snapshot.is_none());
        assert_eq!(
            launch(Arc::clone(&app), "home".into()).await.unwrap(),
            Some(5)
        );
    });
}

#[test]
fn storage_failures_prevent_requests_or_new_cache_publication_until_metadata_is_recovered() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, false);
    run(async {
        configure(&app, "home").await;
        let file = &app.connections_file;
        let backup = file.with_extension("backup");
        std::fs::rename(file, &backup).unwrap();
        std::fs::create_dir(file).unwrap();
        assert!(launch(Arc::clone(&app), "home".into()).await.is_err());
        assert_eq!(source.hits.load(Ordering::SeqCst), 0);
        std::fs::remove_dir(file).unwrap();
        std::fs::rename(&backup, file).unwrap();
        source.reply.lock().unwrap().delay = Duration::from_millis(150);
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        hit(&source, 1).await;
        std::fs::rename(file, &backup).unwrap();
        std::fs::create_dir(file).unwrap();
        complete(&app).await;
        assert!(app.connections.lock().await.dirty);
        assert!(app.connections.lock().await.data["home"].snapshot.is_none());
        assert!(launch(Arc::clone(&app), "home".into()).await.is_err());
        assert_eq!(source.hits.load(Ordering::SeqCst), 1);
        std::fs::remove_dir(file).unwrap();
        std::fs::rename(&backup, file).unwrap();
        clock.store(START + 5, Ordering::SeqCst);
        launch(Arc::clone(&app), "home".into()).await.unwrap();
        complete(&app).await;
        assert!(!app.connections.lock().await.dirty);
        assert!(app.connections.lock().await.data["home"].snapshot.is_some());
    });
}

#[test]
fn data_cache_budget_preserves_existing_successes_when_another_result_will_not_fit() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, false);
    source.reply.lock().unwrap().body = json!({"schema_version":1,"id":"large","fields":[{"id":"text","kind":"text"}],"rows":(0..8).map(|_|json!({"text":"x".repeat(4096)})).collect::<Vec<_>>()});
    run(async {
        for index in 0..17 {
            let id = format!("source-{index}");
            configure(&app, &id).await;
            clock.store(START + index * 5, Ordering::SeqCst);
            launch(Arc::clone(&app), id).await.unwrap();
            complete(&app).await;
        }
        let store = app.connections.lock().await;
        connections::validate_store(&store, &app.connector_catalog).unwrap();
        assert!(store.data["source-0"].snapshot.is_some());
        assert!(store.data["source-16"].snapshot.is_none());
        assert!(
            store.data["source-16"]
                .last_error
                .as_ref()
                .unwrap()
                .contains("budget")
        );
        assert!(std::fs::metadata(&app.connections_file).unwrap().len() < 1024 * 1024);
    });
}

#[test]
fn an_oversized_serialized_state_cannot_replace_a_reloadable_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("state.json");
    std::fs::write(&file, b"{\"previous\":true}\n").unwrap();
    let oversized = json!({"value":"x".repeat(crate::simulator::MAX_JSON_BYTES)});
    assert!(crate::server::persist(&file, &oversized).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"{\"previous\":true}\n");
}

fn provider_app(
    source: &Source,
    id: &str,
    variable: &str,
) -> (Shared, Arc<AtomicU64>, tempfile::TempDir) {
    let (mut app, clock, dir) = support::app(
        support::scene(),
        RefreshSettings::default(),
        support::empty(),
        START + 20,
    );
    let wasm = std::env::var(variable).expect("scripts/check.sh builds provider guests");
    let registration = if id == "rss.headlines" {
        json!({"connector_id":id,"wasm":wasm,"source_setting":"feed_url","origin_interval_secs":2,"stale_after_secs":21600})
    } else {
        json!({"connector_id":id,"wasm":wasm,"source_url":source.url,"origin_interval_secs":2,"stale_after_secs":3600})
    };
    let path = dir.path().join("bindings.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({"schema_version":1,"bindings":[registration]})).unwrap(),
    )
    .unwrap();
    let registry = Registry::load(&path, &app.connector_catalog).unwrap();
    Arc::get_mut(&mut app).unwrap().executors = Some(Arc::new(registry));
    (app, clock, dir)
}
async fn configure_provider(app: &Shared, id: &str, connector: &str, settings: Value) {
    let revision = app.connections.lock().await.revision;
    let request = serde_json::from_value(json!({"base_revision":revision,"connector_id":connector,"label":"Provider test","enabled":true,"poll_interval_secs":1800,"settings":settings})).unwrap();
    let _ = connections::save(State(Arc::clone(app)), RoutePath(id.into()), Json(request))
        .await
        .unwrap();
}

#[test]
fn compiled_weather_results_cache_all_forecasts_and_failures_mark_every_dataset_stale() {
    let source = Source::new();
    source.reply.lock().unwrap().body = serde_json::from_slice(include_bytes!(
        "../../../../../plugins/weather/fixtures/forecast.json"
    ))
    .unwrap();
    let (app, clock, _dir) = provider_app(
        &source,
        "weather.open-meteo",
        "PAPERPHASE_TEST_WEATHER_PLUGIN",
    );
    run(async {
        configure_provider(&app, "weather.home", "weather.open-meteo", json!({})).await;
        launch(Arc::clone(&app), "weather.home".into())
            .await
            .unwrap();
        complete(&app).await;
        {
            let store = app.connections.lock().await;
            let data = &store.data["weather.home"];
            assert!(data.last_error.is_none(), "{:?}", data.last_error);
            assert_eq!(
                data.snapshot.as_ref().unwrap().observed_at,
                Some(1700000000)
            );
            assert_eq!(data.additional_snapshots.len(), 2);
            let persisted: connections::ConnectionStore =
                super::super::read_json(&app.connections_file).unwrap();
            assert_eq!(persisted.data["weather.home"].additional_snapshots.len(), 2);
            assert_eq!(
                connections::merge_snapshots(&store, &app.snapshots)
                    .snapshots
                    .len(),
                3
            );
        }
        clock.store(1700001800, Ordering::SeqCst);
        source.reply.lock().unwrap().body = json!({"error":true});
        launch(Arc::clone(&app), "weather.home".into())
            .await
            .unwrap();
        complete(&app).await;
        let store = app.connections.lock().await;
        let data = &store.data["weather.home"];
        assert_eq!(
            data.snapshot.as_ref().unwrap().dataset.rows[0]["temperature"],
            12.5
        );
        assert!(data.snapshot.as_ref().unwrap().last_error.is_some());
        assert!(
            data.additional_snapshots
                .iter()
                .all(|snapshot| snapshot.last_error.is_some())
        );
    });
}

#[test]
fn compiled_rss_grants_each_saved_origin_and_keeps_ttl_across_manual_fetch_and_restart() {
    let first = Source::new();
    let second = Source::new();
    first.reply.lock().unwrap().raw =
        Some(include_bytes!("../../../../../plugins/rss/fixtures/headlines.xml").to_vec());
    second.reply.lock().unwrap().raw =
        Some(include_bytes!("../../../../../plugins/rss/fixtures/atom.xml").to_vec());
    let (app, clock, _dir) = provider_app(&first, "rss.headlines", "PAPERPHASE_TEST_RSS_PLUGIN");
    run(async {
        configure_provider(
            &app,
            "rss.first",
            "rss.headlines",
            json!({"feed_url":first.url}),
        )
        .await;
        launch(Arc::clone(&app), "rss.first".into()).await.unwrap();
        complete(&app).await;
        {
            let store = app.connections.lock().await;
            assert!(
                store.data["rss.first"].last_error.is_none(),
                "{:?}",
                store.data["rss.first"].last_error
            );
            assert_eq!(
                store.data["rss.first"]
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .dataset
                    .rows
                    .len(),
                3
            );
        }
        assert_eq!(
            launch(Arc::clone(&app), "rss.first".into()).await.unwrap(),
            Some(3600)
        );
        let persisted: connections::ConnectionStore =
            super::super::read_json(&app.connections_file).unwrap();
        assert_eq!(persisted.data["rss.first"].retry_not_before, 1700003600);
        configure_provider(
            &app,
            "rss.second",
            "rss.headlines",
            json!({"feed_url":second.url}),
        )
        .await;
        launch(Arc::clone(&app), "rss.second".into()).await.unwrap();
        complete(&app).await;
        assert_eq!(
            app.connections.lock().await.data["rss.second"]
                .snapshot
                .as_ref()
                .unwrap()
                .dataset
                .rows[0]["title"],
            "Tea & cake"
        );
        clock.store(1700003600, Ordering::SeqCst);
        first.reply.lock().unwrap().raw = Some(b"<rss version=\"2.0\"><channel/></rss>".to_vec());
        launch(Arc::clone(&app), "rss.first".into()).await.unwrap();
        complete(&app).await;
        let store = app.connections.lock().await;
        assert!(
            store.data["rss.first"]
                .snapshot
                .as_ref()
                .unwrap()
                .dataset
                .rows
                .is_empty()
        );
        assert!(store.data["rss.first"].last_error.is_none());
        assert_eq!(first.hits.load(Ordering::SeqCst), 2);
        assert_eq!(second.hits.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn compiled_luas_caches_empty_forecasts_and_keeps_provider_observation_age() {
    let source = Source::new();
    source.reply.lock().unwrap().raw =
        Some(include_bytes!("../../../../../plugins/luas/fixtures/forecast.xml").to_vec());
    let (app, clock, _dir) = provider_app(&source, "transport.luas", "PAPERPHASE_TEST_LUAS_PLUGIN");
    run(async {
        configure_provider(&app, "luas.home", "transport.luas", json!({"stop":"RAN"})).await;
        launch(Arc::clone(&app), "luas.home".into()).await.unwrap();
        complete(&app).await;
        {
            let store = app.connections.lock().await;
            let d = &store.data["luas.home"];
            assert!(d.last_error.is_none(), "{:?}", d.last_error);
            assert_eq!(
                d.snapshot.as_ref().unwrap().dataset.rows[0]["arrival-count"],
                6
            );
            assert_eq!(d.additional_snapshots[0].observed_at, Some(1700000000));
            let persisted: connections::ConnectionStore =
                super::super::read_json(&app.connections_file).unwrap();
            assert_eq!(
                persisted.data["luas.home"].additional_snapshots[0].observed_at,
                Some(1700000000)
            );
        }
        clock.store(1700000060, Ordering::SeqCst);
        // A newly fetched old payload must remain stale in both datasets.
        source.reply.lock().unwrap().raw = Some(
            include_str!("../../../../../plugins/luas/fixtures/empty.xml")
                .replace("22:13:20", "20:13:20")
                .into_bytes(),
        );
        launch(Arc::clone(&app), "luas.home".into()).await.unwrap();
        complete(&app).await;
        {
            let store = app.connections.lock().await;
            let d = &store.data["luas.home"];
            assert!(d.last_error.is_none(), "{:?}", d.last_error);
            assert_eq!(
                d.snapshot.as_ref().unwrap().dataset.rows[0]["arrival-count"],
                0
            );
            assert!(d.additional_snapshots[0].dataset.rows.is_empty());
            assert!(d.snapshot.as_ref().unwrap().is_stale(1700000060));
            assert!(d.additional_snapshots[0].is_stale(1700000060));
        }
        clock.store(1700000120, Ordering::SeqCst);
        source.reply.lock().unwrap().raw = Some(b"<error/>".to_vec());
        launch(Arc::clone(&app), "luas.home".into()).await.unwrap();
        complete(&app).await;
        let store = app.connections.lock().await;
        let d = &store.data["luas.home"];
        assert!(d.snapshot.as_ref().unwrap().last_error.is_some());
        assert!(d.additional_snapshots[0].last_error.is_some());
        assert_eq!(
            d.snapshot.as_ref().unwrap().dataset.rows[0]["service-message"],
            "No services operating"
        );
    });
}

#[test]
fn automatic_connections_share_a_busy_origin_without_starving_later_ids() {
    let source = Source::new();
    let (app, clock, _dir) = setup(&source, false);
    run(async {
        configure(&app, "a").await;
        configure(&app, "b").await;
        let worker = start(Arc::clone(&app));
        hit(&source, 1).await;
        complete(&app).await;
        assert!(app.connections.lock().await.data["a"].snapshot.is_some());
        clock.store(START + 5, Ordering::SeqCst);
        hit(&source, 2).await;
        complete(&app).await;
        assert!(app.connections.lock().await.data["b"].snapshot.is_some());
        clock.store(START + 10, Ordering::SeqCst);
        hit(&source, 3).await;
        complete(&app).await;
        assert_eq!(
            app.connections.lock().await.data["a"].last_attempt_at,
            Some(START + 10)
        );
        worker.abort();
    });
}
