use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use paperphase_runtime::{Dataset, Limits, PluginRuntime, RunConfig};
use serde_json::json;

const DATASET: &str = r#"{"schema_version":1,"id":"test","fields":[{"id":"temperature","kind":"number"}],"rows":[{"temperature":18.5}]}"#;

fn config() -> RunConfig {
    RunConfig::new("http://127.0.0.1:1/unused").unwrap()
}

fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\{byte:02x}")).collect()
}

fn publish_wasm(body: &str, extra: &str, status: i32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"
        (module
          (import "paperphase_v0" "publish" (func $publish (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "{}")
          (func (export "run") (result i32)
            {extra}
            i32.const 0 i32.const {} call $publish drop
            i32.const {status}))
    "#,
        escaped(body.as_bytes()),
        body.len()
    ))
    .unwrap()
}

fn http_wasm(url: Option<&str>, repeat: usize) -> Vec<u8> {
    let url_data = url
        .map(|url| format!(r#"(data (i32.const 0) "{}")"#, escaped(url.as_bytes())))
        .unwrap_or_default();
    let setup = url
        .map(|url| format!("i32.const {} local.set $url_len", url.len()))
        .unwrap_or_else(|| "i32.const 0 i32.const 8192 call $source local.set $url_len".into());
    let calls = (0..repeat).map(|_| "i32.const 0 local.get $url_len i32.const 16384 i32.const 65536 call $get local.set $body_len").collect::<Vec<_>>().join("\n");
    wat::parse_str(format!(
        r#"
        (module
          (import "paperphase_v0" "source_url" (func $source (param i32 i32) (result i32)))
          (import "paperphase_v0" "http_get" (func $get (param i32 i32 i32 i32) (result i32)))
          (import "paperphase_v0" "publish" (func $publish (param i32 i32) (result i32)))
          (memory (export "memory") 2)
          {url_data}
          (func (export "run") (result i32) (local $url_len i32) (local $body_len i32)
            {setup}
            {calls}
            i32.const 16384 local.get $body_len call $publish drop
            i32.const 0))
    "#
    ))
    .unwrap()
}

fn error(runtime: &PluginRuntime, wasm: &[u8], config: &RunConfig) -> String {
    let module = runtime.prepare(wasm).unwrap();
    format!("{:#}", runtime.run(&module, config).unwrap_err())
}

#[test]
fn typed_dataset_and_fresh_invocations() {
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let module = runtime.prepare(&publish_wasm(DATASET, "", 0)).unwrap();
    for _ in 0..3 {
        let result = runtime.run(&module, &config()).unwrap();
        assert_eq!(result.dataset.rows[0]["temperature"], 18.5);
        assert_eq!(result.metrics.http_requests, 0);
        assert!(result.metrics.fuel_consumed > 0);
        assert_eq!(result.metrics.memory_bytes, 65536);
    }
}

#[test]
fn infinite_loop_is_interrupted_and_next_plugin_runs() {
    let runtime = PluginRuntime::new(Limits {
        fuel: 10_000,
        ..Limits::default()
    })
    .unwrap();
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "run") (result i32) (loop $forever br $forever) i32.const 0))"#,
    )
    .unwrap();
    assert!(error(&runtime, &wasm, &config()).contains("fuel"));
    let healthy = runtime.prepare(&publish_wasm(DATASET, "", 0)).unwrap();
    runtime.run(&healthy, &config()).unwrap();
}

#[test]
fn start_function_is_also_fuel_limited() {
    let runtime = PluginRuntime::new(Limits {
        fuel: 10_000,
        ..Limits::default()
    })
    .unwrap();
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func $start (loop $forever br $forever)) (start $start)
        (func (export "run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    assert!(error(&runtime, &wasm, &config()).contains("fuel"));
}

#[test]
fn memory_growth_and_initial_allocation_are_limited() {
    let runtime = PluginRuntime::new(Limits {
        memory_bytes: 65536,
        ..Limits::default()
    })
    .unwrap();
    let grow = publish_wasm(DATASET, "i32.const 1 memory.grow drop", 0);
    assert!(error(&runtime, &grow, &config()).contains("memory"));
    let initial = wat::parse_str(
        r#"(module (memory (export "memory") 2)
        (func (export "run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    assert!(error(&runtime, &initial, &config()).contains("memory"));
}

#[test]
fn tables_are_limited() {
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let wasm = wat::parse_str(
        r#"(module
        (memory (export "memory") 1) (table 1 funcref)
        (func (export "run") (result i32)
          ref.null func i32.const 5000 table.grow drop i32.const 0))"#,
    )
    .unwrap();
    assert!(error(&runtime, &wasm, &config()).contains("table"));
}

#[test]
fn guest_buffers_are_checked() {
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    for (ptr, len) in [(65530, 20), (-1, 20), (0, -1)] {
        let wasm = wat::parse_str(format!(
            r#"(module
          (import "paperphase_v0" "publish" (func $publish (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "run") (result i32) i32.const {ptr} i32.const {len} call $publish))"#
        ))
        .unwrap();
        assert!(error(&runtime, &wasm, &config()).contains("buffer"));
    }
}

#[test]
fn output_is_bounded_and_only_committed_on_success() {
    let runtime = PluginRuntime::new(Limits {
        max_output_bytes: 16,
        ..Limits::default()
    })
    .unwrap();
    assert!(error(&runtime, &publish_wasm(DATASET, "", 0), &config()).contains("byte limit"));
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    assert!(error(&runtime, &publish_wasm(DATASET, "", 1), &config()).contains("failure status"));
    let duplicate = format!("i32.const 0 i32.const {} call $publish drop", DATASET.len());
    assert!(
        error(&runtime, &publish_wasm(DATASET, &duplicate, 0), &config())
            .contains("only one dataset")
    );
}

#[test]
fn datasets_reject_type_and_schema_errors_but_allow_empty_results() {
    let original: serde_json::Value = serde_json::from_str(DATASET).unwrap();
    let mut bad = Vec::new();
    let mut value = original.clone();
    value["rows"][0]["temperature"] = json!("warm");
    bad.push(value);
    let mut value = original.clone();
    value["rows"][0]["other"] = json!(1);
    bad.push(value);
    let mut value = original.clone();
    let field = value["fields"][0].clone();
    value["fields"].as_array_mut().unwrap().push(field);
    bad.push(value);
    let mut value = original.clone();
    value["schema_version"] = json!(99);
    bad.push(value);
    let mut value = original.clone();
    value["id"] = json!("");
    bad.push(value);
    let mut value = original.clone();
    value["rows"][0]["temperature"] = json!({"nested": true});
    bad.push(value);
    for value in bad {
        let dataset: Dataset = serde_json::from_value(value).unwrap();
        assert!(dataset.validate().is_err());
    }
    let mut empty = original;
    empty["rows"] = json!([]);
    serde_json::from_value::<Dataset>(empty)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn wasi_and_unknown_imports_are_unavailable() {
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let wasm = wat::parse_str(
        r#"(module
      (import "wasi_snapshot_preview1" "random_get" (func (param i32 i32) (result i32)))
      (memory (export "memory") 1) (func (export "run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    assert!(error(&runtime, &wasm, &config()).contains("unknown import"));
}

#[test]
fn rejects_non_wasm_oversized_modules_and_foreign_runtimes() {
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    assert!(runtime.prepare(b"not portable wasm").is_err());
    assert!(
        runtime
            .prepare(&vec![0; Limits::default().max_module_bytes + 1])
            .is_err()
    );
    let other = PluginRuntime::new(Limits::default()).unwrap();
    let module = other.prepare(&publish_wasm(DATASET, "", 0)).unwrap();
    assert!(
        format!("{:#}", runtime.run(&module, &config()).unwrap_err()).contains("different runtime")
    );
}

#[test]
fn http_uses_the_configured_source_and_resets_request_quotas() {
    let server = Server::ok(DATASET.as_bytes());
    let config = RunConfig::new(&server.url).unwrap();
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let module = runtime.prepare(&http_wasm(None, 1)).unwrap();
    for _ in 0..2 {
        let result = runtime.run(&module, &config).unwrap();
        assert_eq!(result.dataset.rows[0]["temperature"], 18.5);
        assert_eq!(result.metrics.http_requests, 1);
    }
    assert_eq!(server.hits(), 2);
}

#[test]
fn denied_origin_never_receives_a_request() {
    let source = Server::ok(DATASET.as_bytes());
    let denied = Server::ok(DATASET.as_bytes());
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let error = error(
        &runtime,
        &http_wasm(Some(&denied.url), 1),
        &RunConfig::new(&source.url).unwrap(),
    );
    assert!(error.contains("origin is not granted"));
    assert_eq!(denied.hits(), 0);
    assert_eq!(source.hits(), 0);
}

#[test]
fn redirects_are_not_followed() {
    let target = Server::ok(DATASET.as_bytes());
    let redirect = Server::new(
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            target.url
        )
        .into_bytes(),
        Duration::ZERO,
    );
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    assert!(
        error(
            &runtime,
            &http_wasm(None, 1),
            &RunConfig::new(&redirect.url).unwrap()
        )
        .contains("status 302")
    );
    assert_eq!(target.hits(), 0);
}

#[test]
fn request_quota_stops_additional_network_activity() {
    let source = Server::ok(DATASET.as_bytes());
    let runtime = PluginRuntime::new(Limits {
        max_requests: 1,
        ..Limits::default()
    })
    .unwrap();
    assert!(
        error(
            &runtime,
            &http_wasm(None, 2),
            &RunConfig::new(&source.url).unwrap()
        )
        .contains("quota exceeded")
    );
    assert_eq!(source.hits(), 1);
}

#[test]
fn streaming_body_limit_is_enforced_without_content_length() {
    let body = "x".repeat(1024);
    let response = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
        body.len(),
        body
    );
    let source = Server::new(response.into_bytes(), Duration::ZERO);
    let runtime = PluginRuntime::new(Limits {
        max_response_bytes: 32,
        ..Limits::default()
    })
    .unwrap();
    assert!(
        error(
            &runtime,
            &http_wasm(None, 1),
            &RunConfig::new(&source.url).unwrap()
        )
        .contains("response byte limit")
    );
}

#[test]
fn stalled_headers_and_bodies_time_out_and_runtime_recovers() {
    let source = Server::new(Vec::new(), Duration::from_millis(200));
    let runtime = PluginRuntime::new(Limits {
        request_timeout: Duration::from_millis(40),
        ..Limits::default()
    })
    .unwrap();
    assert!(
        error(
            &runtime,
            &http_wasm(None, 1),
            &RunConfig::new(&source.url).unwrap()
        )
        .contains("timed out")
    );
    let body_stall = Server::new(
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n".to_vec(),
        Duration::from_millis(200),
    );
    assert!(
        error(
            &runtime,
            &http_wasm(None, 1),
            &RunConfig::new(&body_stall.url).unwrap()
        )
        .contains("timed out")
    );
    let healthy = runtime.prepare(&publish_wasm(DATASET, "", 0)).unwrap();
    runtime.run(&healthy, &config()).unwrap();
}

#[test]
fn source_url_rejects_unsupported_forms() {
    for source in [
        "file:///etc/passwd",
        "http://user:password@localhost/",
        "http://localhost/#fragment",
    ] {
        assert!(RunConfig::new(source).is_err());
    }
}

#[test]
fn settings_are_bounded_and_guest_buffers_are_checked() {
    let config = config()
        .with_settings(&std::collections::BTreeMap::from([(
            "secret".into(),
            json!("do-not-debug-this"),
        )]))
        .unwrap();
    assert!(!format!("{config:?}").contains("do-not-debug-this"));
    assert!(
        config
            .clone()
            .with_settings(&std::collections::BTreeMap::from([(
                "large".into(),
                json!("x".repeat(16 * 1024))
            )]))
            .is_err()
    );
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    for (ptr, capacity, expected) in [
        (0, 1, "too small"),
        (65536, 16384, "buffer"),
        (-1, 16384, "buffer"),
        (0, 16385, "buffer"),
    ] {
        let wasm = wat::parse_str(format!(r#"(module (import "paperphase_v0" "settings_json" (func $settings (param i32 i32) (result i32))) (memory (export "memory") 1) (func (export "run") (result i32) i32.const {ptr} i32.const {capacity} call $settings drop i32.const 0))"#)).unwrap();
        assert!(error(&runtime, &wasm, &config).contains(expected));
    }
}

#[test]
fn provider_retry_hints_are_available_on_failure_and_isolated_per_invocation() {
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let module = runtime.prepare(&http_wasm(None, 1)).unwrap();
    for (status, hint, minimum, maximum) in [
        (429, "120".to_owned(), 120, 120),
        (
            503,
            httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(120)),
            100,
            120,
        ),
        (429, "not-a-delay".into(), 0, 0),
        (503, "Sun, 06 Nov 1994 08:49:37 GMT".into(), 0, 0),
    ] {
        let source = Server::new(format!("HTTP/1.1 {status} Unavailable\r\nRetry-After: {hint}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes(), Duration::ZERO);
        let attempt = runtime.run_observed(&module, &RunConfig::new(&source.url).unwrap());
        assert!(attempt.result.is_err());
        assert!(attempt.provider_backoff);
        assert!((minimum..=maximum).contains(&attempt.retry_after_secs.unwrap_or(0)));
    }
    let source = Server::ok(DATASET.as_bytes());
    let attempt = runtime.run_observed(&module, &RunConfig::new(&source.url).unwrap());
    assert!(attempt.result.is_ok());
    assert!(!attempt.provider_backoff);
    assert!(attempt.retry_after_secs.is_none());
}

#[test]
#[ignore = "build the Rust guest and set PAPERPHASE_TEST_PLUGIN; scripts/check.sh does this"]
fn compiled_rust_plugin_fetches_and_transforms_data() {
    let wasm =
        std::fs::read(std::env::var("PAPERPHASE_TEST_PLUGIN").expect("set PAPERPHASE_TEST_PLUGIN"))
            .unwrap();
    let source = Server::ok(br#"{"temperature_c":18.5}"#);
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let module = runtime.prepare(&wasm).unwrap();
    let result = runtime
        .run(&module, &RunConfig::new(&source.url).unwrap())
        .unwrap();
    assert_eq!(result.dataset.id, "current-weather");
    assert_eq!(result.dataset.rows[0]["temperature"], 18.5);
    assert_eq!(result.metrics.http_requests, 1);
    let config = RunConfig::new(&source.url)
        .unwrap()
        .with_settings(&std::collections::BTreeMap::from([
            ("location".into(), json!("Cork")),
            ("units".into(), json!("imperial")),
        ]))
        .unwrap();
    let result = runtime.run(&module, &config).unwrap();
    assert_eq!(result.dataset.rows[0]["location"], "Cork");
    assert_eq!(result.dataset.rows[0]["display-temperature"], "65.3 °F");
    let empty = Server::ok(br#"{"temperature_c":null}"#);
    assert!(
        runtime
            .run(&module, &RunConfig::new(&empty.url).unwrap())
            .unwrap()
            .dataset
            .rows
            .is_empty()
    );
}

struct Server {
    request: Arc<Mutex<Vec<u8>>>,
    url: String,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    fn ok(body: &[u8]) -> Self {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        Self::new(response, Duration::ZERO)
    }

    fn new(response: Vec<u8>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/data", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let requests = Arc::clone(&hits);
        let captured = Arc::new(Mutex::new(Vec::new()));
        let capture = Arc::clone(&captured);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        requests.fetch_add(1, Ordering::Relaxed);
                        stream
                            .set_read_timeout(Some(Duration::from_millis(250)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_millis(250)))
                            .unwrap();
                        let mut request = [0; 8192];
                        let length = stream.read(&mut request).unwrap_or(0);
                        *capture.lock().unwrap() = request[..length].to_vec();
                        let _ = stream.write_all(&response);
                        // Keep an incomplete response open to test body timeouts too.
                        thread::sleep(delay);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::park_timeout(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            request: captured,
            url,
            hits,
            stop,
            worker: Some(worker),
        }
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            worker.join().unwrap();
        }
    }
}

#[test]
fn publication_metadata_is_atomic_bounded_and_compatible_with_legacy_publish() {
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let dataset: Dataset = serde_json::from_str(DATASET).unwrap();
    let mut extra = dataset.clone();
    extra.id = "hourly".into();
    let publication = json!({"dataset":dataset,"observed_at":123,"min_poll_secs":3600,"additional_datasets":[extra],"additional_observed_at":{"hourly":120}});
    // Reuse the minimal boundary guest while selecting the new import.
    fn metadata_wasm(value: &serde_json::Value, status: i32) -> Vec<u8> {
        let body = value.to_string();
        wat::parse_str(format!(r#"(module
          (import "paperphase_v0" "publish_with_metadata" (func $publish (param i32 i32) (result i32)))
          (memory (export "memory") 2)
          (data (i32.const 0) "{}")
          (func (export "run") (result i32) i32.const 0 i32.const {} call $publish drop i32.const {}))"#, escaped(body.as_bytes()), body.len(), status)).unwrap()
    }
    let plugin = runtime.prepare(&metadata_wasm(&publication, 0)).unwrap();
    let result = runtime.run(&plugin, &config()).unwrap();
    assert_eq!(result.observed_at, Some(123));
    assert_eq!(result.min_poll_secs, Some(3600));
    assert_eq!(result.additional_datasets[0].id, "hourly");
    assert_eq!(result.additional_observed_at.get("hourly"), Some(&120));
    assert!(
        runtime
            .run(
                &runtime.prepare(&metadata_wasm(&publication, 1)).unwrap(),
                &config()
            )
            .is_err()
    );
    let mut duplicate = publication.clone();
    duplicate["additional_datasets"][0]["id"] = json!("test");
    assert!(
        runtime
            .run(
                &runtime.prepare(&metadata_wasm(&duplicate, 0)).unwrap(),
                &config()
            )
            .is_err()
    );
    let mut invalid_hint = publication.clone();
    invalid_hint["min_poll_secs"] = json!(604801);
    assert!(
        runtime
            .run(
                &runtime.prepare(&metadata_wasm(&invalid_hint, 0)).unwrap(),
                &config()
            )
            .is_err()
    );
    let mut unknown_observation = publication.clone();
    unknown_observation["additional_observed_at"] = json!({"missing-dataset":120});
    assert!(
        runtime
            .run(
                &runtime
                    .prepare(&metadata_wasm(&unknown_observation, 0))
                    .unwrap(),
                &config()
            )
            .is_err()
    );
    let legacy = runtime
        .run(
            &runtime.prepare(&publish_wasm(DATASET, "", 0)).unwrap(),
            &config(),
        )
        .unwrap();
    assert!(legacy.additional_datasets.is_empty());
    assert!(legacy.observed_at.is_none());
}

#[test]
#[ignore = "scripts/check.sh builds the compiled weather guest"]
fn compiled_weather_plugin_publishes_forecasts_with_observation_metadata() {
    let source = Server::ok(include_bytes!(
        "../../../plugins/weather/fixtures/forecast.json"
    ));
    let runtime = PluginRuntime::new(Limits {
        max_requests: 1,
        ..Limits::default()
    })
    .unwrap();
    let wasm = std::fs::read(
        std::env::var("PAPERPHASE_TEST_WEATHER_PLUGIN").expect("build weather guest"),
    )
    .unwrap();
    let module = runtime.prepare(&wasm).unwrap();
    let catalog: paperphase_core::ConnectorCatalog =
        serde_json::from_str(include_str!("../../../examples/simulator/connectors.json")).unwrap();
    let settings = catalog
        .get("weather.open-meteo")
        .unwrap()
        .resolve(&Default::default(), None)
        .unwrap();
    let config = RunConfig::new(&source.url)
        .unwrap()
        .with_settings(&settings)
        .unwrap();
    let result = runtime.run(&module, &config).unwrap();
    assert_eq!(result.dataset.rows[0]["display-temperature"], "12.5 °C");
    assert_eq!(result.additional_datasets.len(), 2);
    assert_eq!(result.additional_datasets[0].id, "hourly");
    assert_eq!(result.additional_datasets[1].id, "daily");
    assert_eq!(result.observed_at, Some(1700000000));
    assert_eq!(result.metrics.http_requests, 1);
    assert!(result.metrics.fuel_consumed < 10_000_000);
    let mut long: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../plugins/weather/fixtures/forecast.json"
    ))
    .unwrap();
    for (field, values) in long["hourly"].as_object_mut().unwrap() {
        let original = values.as_array().unwrap().clone();
        *values = if field == "time" {
            json!(
                (0..48)
                    .map(|index| 1700002800_u64 + index * 3600)
                    .collect::<Vec<_>>()
            )
        } else {
            json!(
                (0..48)
                    .map(|index| original[index % original.len()].clone())
                    .collect::<Vec<_>>()
            )
        };
    }
    let long_source = Server::ok(&serde_json::to_vec(&long).unwrap());
    let mut long_settings = settings;
    long_settings.insert("forecast_hours".into(), json!(48));
    let result = runtime
        .run(
            &module,
            &RunConfig::new(&long_source.url)
                .unwrap()
                .with_settings(&long_settings)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(result.additional_datasets[0].rows.len(), 48);
    assert!(result.metrics.fuel_consumed < 10_000_000);
}

#[test]
#[ignore = "scripts/check.sh builds the compiled RSS guest"]
fn compiled_rss_plugin_extracts_headlines_and_enforces_feed_ttl() {
    let source = Server::ok(include_bytes!(
        "../../../plugins/rss/fixtures/headlines.xml"
    ));
    let runtime = PluginRuntime::new(Limits {
        max_requests: 1,
        ..Limits::default()
    })
    .unwrap();
    let wasm = std::fs::read(std::env::var("PAPERPHASE_TEST_RSS_PLUGIN").expect("build RSS guest"))
        .unwrap();
    let module = runtime.prepare(&wasm).unwrap();
    let settings =
        serde_json::from_value(json!({"label":"Test feed","feed_url":source.url,"max_items":3}))
            .unwrap();
    let config = RunConfig::new(&source.url)
        .unwrap()
        .with_settings(&settings)
        .unwrap();
    let result = runtime.run(&module, &config).unwrap();
    assert_eq!(result.dataset.rows.len(), 3);
    assert_eq!(result.min_poll_secs, Some(3600));
    assert_eq!(result.metrics.http_requests, 1);
    assert!(result.metrics.fuel_consumed < 10_000_000);
    let atom = Server::ok(include_bytes!("../../../plugins/rss/fixtures/atom.xml"));
    let config = RunConfig::new(&atom.url)
        .unwrap()
        .with_settings(&settings)
        .unwrap();
    assert_eq!(
        runtime.run(&module, &config).unwrap().dataset.rows[0]["title"],
        "Tea & cake"
    );
    let bad = Server::ok(b"<not-a-feed>");
    assert!(
        runtime
            .run(
                &module,
                &RunConfig::new(&bad.url)
                    .unwrap()
                    .with_settings(&settings)
                    .unwrap()
            )
            .is_err()
    );
}

#[test]
#[ignore = "scripts/check.sh builds the compiled Luas guest"]
fn compiled_luas_plugin_uses_one_request_and_preserves_times_for_both_datasets() {
    let source = Server::ok(include_bytes!(
        "../../../plugins/luas/fixtures/forecast.xml"
    ));
    let runtime = PluginRuntime::new(Limits {
        max_requests: 1,
        ..Limits::default()
    })
    .unwrap();
    let module = runtime
        .prepare(
            &std::fs::read(std::env::var("PAPERPHASE_TEST_LUAS_PLUGIN").expect("build Luas guest"))
                .unwrap(),
        )
        .unwrap();
    let catalog: paperphase_core::ConnectorCatalog =
        serde_json::from_str(include_str!("../../../examples/simulator/connectors.json")).unwrap();
    let settings = catalog
        .get("transport.luas")
        .unwrap()
        .resolve(
            &serde_json::from_value(json!({"stop":"RAN"})).unwrap(),
            None,
        )
        .unwrap();
    let result = runtime
        .run(
            &module,
            &RunConfig::new(&source.url)
                .unwrap()
                .with_settings(&settings)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(result.dataset.rows[0]["stop-name"], "Ranelagh");
    assert_eq!(result.dataset.rows[0]["arrival-count"], 6);
    assert_eq!(result.additional_datasets[0].rows.len(), 6);
    assert_eq!(result.observed_at, Some(1700000000));
    assert_eq!(result.additional_observed_at["arrivals"], 1700000000);
    assert_eq!(result.metrics.http_requests, 1);
    assert!(result.metrics.fuel_consumed < 10_000_000);
    let bad = Server::ok(b"<html>Provider error</html>");
    assert!(
        runtime
            .run(
                &module,
                &RunConfig::new(&bad.url)
                    .unwrap()
                    .with_settings(&settings)
                    .unwrap()
            )
            .is_err()
    );
}

#[test]
fn authentication_headers_are_host_granted_redacted_and_exact_origin_only() {
    let source = Server::ok(DATASET.as_bytes());
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let config = RunConfig::new(&source.url)
        .unwrap()
        .with_secret_header("x-api-key", "fixture-secret-key")
        .unwrap();
    assert!(!format!("{config:?}").contains("fixture-secret-key"));
    let module = runtime.prepare(&http_wasm(None, 1)).unwrap();
    runtime.run(&module, &config).unwrap();
    assert!(
        String::from_utf8_lossy(&source.request.lock().unwrap())
            .to_lowercase()
            .contains("x-api-key: fixture-secret-key")
    );
    // A later invocation without a grant must not inherit credentials.
    runtime
        .run(&module, &RunConfig::new(&source.url).unwrap())
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&source.request.lock().unwrap())
            .to_lowercase()
            .contains("x-api-key")
    );
    let other = Server::ok(DATASET.as_bytes());
    assert!(
        runtime
            .run(
                &runtime.prepare(&http_wasm(Some(&other.url), 1)).unwrap(),
                &config
            )
            .is_err()
    );
    assert_eq!(other.hits(), 0);
    for (name, value) in [
        ("host", "example.org"),
        ("x-api-key", "bad\r\nInjected: value"),
        ("x-api-key", ""),
    ] {
        assert!(
            RunConfig::new(&source.url)
                .unwrap()
                .with_secret_header(name, value)
                .is_err()
        );
    }
}

#[test]
#[ignore = "scripts/check.sh builds the Dublin Bus guest"]
fn compiled_bus_guest_queries_granted_tables_and_publishes_multiple_stops() {
    let source = Server::ok(include_bytes!(
        "../../../plugins/dublin-bus/fixtures/sparse-updates.pb"
    ));
    let mut data = paperphase_runtime::ReferenceData::default();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../plugins/dublin-bus/fixtures/reference.json"
    ))
    .unwrap();
    for (table, rows) in fixture.as_object().unwrap() {
        for (key, row) in rows.as_object().unwrap() {
            data.insert(
                table,
                key,
                &serde_json::from_value::<Vec<String>>(row.clone()).unwrap(),
            )
            .unwrap();
        }
    }
    data.insert(
        "calendar",
        "daily",
        &["1", "1", "1", "1", "1", "1", "1", "20230101", "20300101"].map(String::from),
    )
    .unwrap();
    data.insert(
        "trips",
        "trip-scheduled",
        &["route-11", "City Centre", "daily"].map(String::from),
    )
    .unwrap();
    data.append(
        "times",
        "8220DB002813|0",
        &["trip-scheduled", "34", "22:16:00", "22:16:00", "0"].map(String::from),
    )
    .unwrap();
    let runtime = PluginRuntime::new(Limits {
        max_requests: 1,
        ..Limits::default()
    })
    .unwrap();
    let module = runtime
        .prepare(
            &std::fs::read(
                std::env::var("PAPERPHASE_TEST_DUBLIN_BUS_PLUGIN").expect("build Dublin Bus guest"),
            )
            .unwrap(),
        )
        .unwrap();
    let settings=serde_json::from_value(json!({"stop_numbers":"2813, 2","routes":"","max_arrivals":3,"api_key":"fixture-secret-key"})).unwrap();
    let config = RunConfig::new(&source.url)
        .unwrap()
        .with_settings(&settings)
        .unwrap()
        .with_secret_header("x-api-key", "fixture-secret-key")
        .unwrap()
        .with_references(Arc::new(data))
        .with_work_budget(8 * 1024 * 1024, 500_000_000)
        .unwrap();
    let result = runtime.run(&module, &config).unwrap();
    assert_eq!(
        result.additional_datasets[0].rows[0]["expected-at"],
        1700000100_u64
    );
    assert_eq!(result.additional_datasets[0].rows[0]["due-label"], "2 min");
    let scheduled = result.additional_datasets[0]
        .rows
        .iter()
        .find(|row| row["status"] == "scheduled")
        .expect("compiled guest publishes scheduled fallback");
    assert_eq!(scheduled["due-label"], "Scheduled 22:16");
    assert!(scheduled["expected-at"].is_null());
    assert!(scheduled["due-minutes"].is_null());
    assert_eq!(scheduled["scheduled-at"], 1700000160_u64);
    assert_eq!(result.metrics.http_requests, 1);
    assert_eq!(source.hits(), 1);
    assert_eq!(result.dataset.rows.len(), 2);
    assert_eq!(result.dataset.rows[0]["stop-name"], "Ranelagh Road");
    assert_eq!(result.dataset.rows[1]["stop-name"], "Parnell Square West");
    assert_eq!(result.observed_at, Some(1700000000));
    assert_eq!(result.additional_observed_at["arrivals"], 1700000000);
    assert!(result.metrics.fuel_consumed < 500_000_000);
    assert!(result.metrics.memory_bytes <= 32 * 1024 * 1024);
    assert!(
        String::from_utf8_lossy(&source.request.lock().unwrap())
            .to_lowercase()
            .contains("x-api-key: fixture-secret-key")
    );
    let without_reference = RunConfig::new(&source.url)
        .unwrap()
        .with_settings(&settings)
        .unwrap();
    assert!(runtime.run(&module, &without_reference).is_err());
    assert_eq!(
        source.hits(),
        1,
        "Missing reference data must prevent upstream requests"
    );
}

#[test]
fn reference_tables_validate_keys_budgets_and_cache_deserialisation() {
    let mut data = paperphase_runtime::ReferenceData::default();
    data.insert("stops", "2813", &["Ranelagh Road".into()])
        .unwrap();
    assert!(
        data.insert("stops", "2813", &["Different stop".into()])
            .is_err()
    );
    assert!(data.insert("../secrets", "2813", &[]).is_err());
    assert!(data.insert("stops", "bad\nkey", &[]).is_err());
    let mut restored: paperphase_runtime::ReferenceData =
        serde_json::from_slice(&serde_json::to_vec(&data).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.rows(), 1);
    assert_eq!(restored.get("stops", "2813"), Some("[\"Ranelagh Road\"]"));
    for (bytes, fuel) in [
        (64 * 1024 - 1, 10_000_000),
        (8 * 1024 * 1024 + 1, 10_000_000),
        (64 * 1024, 500_000_001),
    ] {
        assert!(config().with_work_budget(bytes, fuel).is_err());
    }
}

#[test]
fn reference_lookup_quota_bounds_native_work_per_invocation() {
    let mut data = paperphase_runtime::ReferenceData::default();
    data.insert("stops", "2813", &["Ranelagh Road".into()])
        .unwrap();
    let config = config().with_references(Arc::new(data));
    let runtime = PluginRuntime::new(Limits::default()).unwrap();
    let wasm=wat::parse_str(r#"(module
  (import "paperphase_v0" "reference_lookup" (func $lookup (param i32 i32 i32 i32 i32 i32)(result i32)))
  (memory (export "memory") 1)(data(i32.const 0) "stops")(data(i32.const 8) "2813")
  (func(export "run")(result i32)(local $count i32)
   (loop $next i32.const 0 i32.const 5 i32.const 8 i32.const 4 i32.const 64 i32.const 4096 call $lookup drop
    local.get $count i32.const 1 i32.add local.tee $count i32.const 4097 i32.lt_u br_if $next)
   i32.const 0))"#).unwrap();
    assert!(error(&runtime, &wasm, &config).contains("reference lookup quota exceeded"));
    assert!(
        runtime
            .run(
                &runtime.prepare(&publish_wasm(DATASET, "", 0)).unwrap(),
                &config
            )
            .is_ok()
    );
}
