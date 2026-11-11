use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::blocking::Client;
use serde_json::{Value, json};
mod fetching_support;

struct Server {
    child: Child,
    url: String,
    client: Client,
}
impl Server {
    fn start(state: &std::path::Path) -> Self {
        Self::start_with_definitions(state, None)
    }
    fn start_with_definitions(
        state: &std::path::Path,
        definitions: Option<&std::path::Path>,
    ) -> Self {
        Self::start_options(state, definitions, None, Some(1700000000))
    }
    fn start_options(
        state: &std::path::Path,
        definitions: Option<&std::path::Path>,
        bindings: Option<&std::path::Path>,
        now: Option<u64>,
    ) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut command = Command::new(env!("CARGO_BIN_EXE_paperphase"));
        command
            .args(["serve", "--listen", "127.0.0.1:0", "--state-file"])
            .arg(state);
        if let Some(path) = definitions {
            command.arg("--connector-definitions").arg(path);
        }
        if let Some(path) = bindings {
            command.arg("--connector-bindings").arg(path);
        }
        if let Some(now) = now {
            command.arg("--now").arg(now.to_string());
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = sender.send(line);
        });
        let line = receiver
            .recv_timeout(Duration::from_secs(15))
            .unwrap_or_else(|error| {
                let _ = child.kill();
                let _ = child.wait();
                panic!("service did not start: {error}");
            });
        let url = line
            .trim()
            .strip_prefix("Paperphase editor: ")
            .expect("startup URL")
            .to_owned();
        Self {
            child,
            url,
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }
    fn state(&self) -> Value {
        self.client
            .get(format!("{}/api/state", self.url))
            .send()
            .unwrap()
            .json()
            .unwrap()
    }
    fn png(&self) -> Vec<u8> {
        self.client
            .get(format!("{}/api/display.png", self.url))
            .send()
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec()
    }
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connection_request(revision: u64) -> Value {
    json!({"base_revision":revision,"connector_id":"demo.weather","label":"Home","enabled":true,"poll_interval_secs":300,"settings":{"location":"Dublin","test_secret":"session-private-value"}})
}

#[test]
fn connection_settings_secrets_and_removal_survive_restart_without_affecting_layouts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let file = path.with_extension("connections.json");
    let mut server = Server::start(&path);
    let original = server.png();
    let url = format!("{}/api/connections/home", server.url);
    let response = server
        .client
        .put(&url)
        .json(&connection_request(0))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    let public = response.text().unwrap();
    assert!(!public.contains("session-private-value"));
    let public: Value = serde_json::from_str(&public).unwrap();
    assert_eq!(
        public["connection"]["secret_present"],
        json!(["test_secret"])
    );
    assert!(
        public["connection"]["settings"]
            .get("test_secret")
            .is_none()
    );
    for endpoint in [
        "/api/state",
        "/api/connections",
        "/api/connectors",
        "/api/catalog",
    ] {
        assert!(
            !server
                .client
                .get(format!("{}{endpoint}", server.url))
                .send()
                .unwrap()
                .text()
                .unwrap()
                .contains("session-private-value")
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    server.stop();
    server = Server::start(&path);
    let mut request = connection_request(1);
    request["settings"] = json!({"location":"Cork"});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/connections/home", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
    let disk: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(
        disk["connections"][0]["settings"]["test_secret"],
        "session-private-value"
    );
    request["base_revision"] = json!(2);
    request["settings"]["test_secret"] = json!("replacement-value");
    assert_eq!(
        server
            .client
            .put(format!("{}/api/connections/home", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
    request["base_revision"] = json!(3);
    request["settings"]["test_secret"] = Value::Null;
    let cleared: Value = server
        .client
        .put(format!("{}/api/connections/home", server.url))
        .json(&request)
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(cleared["connection"]["secret_present"], json!([]));
    assert_eq!(server.state()["revision"], 0);
    assert_eq!(server.png(), original);
    let removed = server
        .client
        .delete(format!("{}/api/connections/home", server.url))
        .json(&json!({"base_revision":4}))
        .send()
        .unwrap();
    assert_eq!(removed.status(), 200);
    let list: Value = server
        .client
        .get(format!("{}/api/connections", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(list["revision"], 5);
    assert_eq!(list["connections"], json!([]));
}

#[test]
fn connection_conflicts_validation_and_failed_storage_leave_the_previous_record_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let file = path.with_extension("connections.json");
    let server = Server::start(&path);
    let mut workers = Vec::new();
    for _ in 0..2 {
        let client = server.client.clone();
        let url = format!("{}/api/connections/home", server.url);
        workers.push(std::thread::spawn(move || {
            client
                .put(url)
                .json(&connection_request(0))
                .send()
                .unwrap()
                .status()
                .as_u16()
        }));
    }
    let mut statuses: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    let disk = std::fs::read(&file).unwrap();
    for (field, value) in [
        ("poll_interval_secs", json!(29)),
        ("connector_id", json!("unknown")),
        ("label", json!(" ")),
    ] {
        let mut request = connection_request(1);
        request[field] = value;
        assert_eq!(
            server
                .client
                .put(format!("{}/api/connections/home", server.url))
                .json(&request)
                .send()
                .unwrap()
                .status(),
            422
        );
    }
    let mut request = connection_request(1);
    request["settings"]["test_secret"] = json!(["never-echo-private"]);
    let error = server
        .client
        .put(format!("{}/api/connections/home", server.url))
        .json(&request)
        .send()
        .unwrap();
    assert_eq!(error.status(), 422);
    assert!(!error.text().unwrap().contains("never-echo-private"));
    assert_eq!(std::fs::read(&file).unwrap(), disk);
    assert_eq!(
        server
            .client
            .put(format!("{}/api/connections/home", server.url))
            .header("Origin", "https://unrelated.example")
            .json(&connection_request(1))
            .send()
            .unwrap()
            .status(),
        403
    );
    let backup = file.with_extension("backup");
    std::fs::rename(&file, &backup).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert_eq!(
        server
            .client
            .put(format!("{}/api/connections/home", server.url))
            .json(&connection_request(1))
            .send()
            .unwrap()
            .status(),
        500
    );
    let list: Value = server
        .client
        .get(format!("{}/api/connections", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(list["revision"], 1);
    assert_eq!(std::fs::read(&backup).unwrap(), disk);
    std::fs::remove_dir(&file).unwrap();
    std::fs::rename(backup, &file).unwrap();
}

#[test]
fn alternate_connector_definitions_are_loaded_and_stored_records_are_validated_against_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let definitions = dir.path().join("descriptors.json");
    let descriptor = json!({"schema_version":1,"connectors":[{"id":"example.rss","name":"Example feed","description":"Settings for a custom source","min_poll_secs":60,"default_poll_secs":600,"settings":[{"id":"feed_name","label":"Feed name","required":true,"input":{"type":"text","max_bytes":64}}]}]});
    std::fs::write(&definitions, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    let mut server = Server::start_with_definitions(&path, Some(&definitions));
    let catalog: Value = server
        .client
        .get(format!("{}/api/connectors", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(
        catalog,
        serde_json::to_value(
            serde_json::from_value::<paperphase_core::ConnectorCatalog>(descriptor).unwrap()
        )
        .unwrap()
    );
    let request = json!({"base_revision":0,"connector_id":"example.rss","label":"News","enabled":true,"poll_interval_secs":600,"settings":{"feed_name":"Local headlines"}});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/connections/news", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
    server.stop();
    let restarted = Server::start_with_definitions(&path, Some(&definitions));
    let list: Value = restarted
        .client
        .get(format!("{}/api/connections", restarted.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(
        list["connections"][0]["settings"]["feed_name"],
        "Local headlines"
    );
}

fn wait_connection(server: &Server, id: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let state: Value = server
            .client
            .get(format!("{}/api/connections", server.url))
            .send()
            .unwrap()
            .json()
            .unwrap();
        if let Some(connection) = state["connections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|connection| connection["id"] == id)
            && predicate(connection)
        {
            return connection.clone();
        }
        assert!(
            Instant::now() < until,
            "connection did not reach expected state: {state}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn temperature_connection(server: &Server, id: &str, poll: u64) {
    let state: Value = server
        .client
        .get(format!("{}/api/connections", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let request = json!({"base_revision":state["revision"],"connector_id":"example.temperature","label":"Balcony","enabled":true,"poll_interval_secs":poll,"settings":{"location":"Balcony","units":"imperial"}});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/connections/{id}", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
}
fn wait_idle(server: &Server) {
    let until = Instant::now() + Duration::from_secs(8);
    while server.state()["phase"] != "idle" {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "scripts/check.sh supplies the compiled guest"]
fn compiled_connector_settings_flow_into_persisted_data_bindings_and_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let source = fetching_support::Observation::start();
    let bindings = fetching_support::registration(dir.path(), &source.url);
    let mut server = Server::start_options(&path, None, Some(&bindings), Some(1700000000));
    temperature_connection(&server, "home", 300);
    let response = server
        .client
        .post(format!("{}/api/connections/home/fetch", server.url))
        .send()
        .unwrap();
    assert!([202, 429].contains(&response.status().as_u16()));
    let record = wait_connection(&server, "home", |connection| {
        connection["fetch"]["status"] == "ready"
    });
    assert_eq!(record["fetch"]["last_success_at"], 1700000000);
    assert_eq!(source.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    let cooldown = server
        .client
        .post(format!("{}/api/connections/home/fetch", server.url))
        .send()
        .unwrap();
    assert_eq!(cooldown.status(), 429);
    assert_eq!(cooldown.headers()["retry-after"], "30");
    let catalog: Value = server
        .client
        .get(format!("{}/api/catalog", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(
        catalog
            .as_array()
            .unwrap()
            .iter()
            .any(|dataset| dataset["connection_id"] == "home"
                && dataset["connection_label"] == "Balcony")
    );
    let state = server.state();
    let before = server.png();
    let mut layout = state["layout"].clone();
    layout["components"][0]["blocks"][0]["source"] = json!({"kind":"field","binding":{"connection_id":"home","dataset_id":"current-weather","field_id":"display-temperature"}});
    let request = json!({"base_revision":state["revision"],"layout":layout,"refresh_settings":{"quiet_hours":{"timezone":"Europe/Dublin","start":"22:00","end":"07:00"}}});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
    assert_eq!(server.state()["phase"], "paused");
    assert_eq!(server.png(), before);
    let preview: Value = server
        .client
        .post(format!("{}/api/preview", server.url))
        .json(&layout)
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(preview["components"][0]["state"], "fresh");
    let disk: Value =
        serde_json::from_slice(&std::fs::read(path.with_extension("connections.json")).unwrap())
            .unwrap();
    assert_eq!(
        disk["data"]["home"]["snapshot"]["dataset"]["rows"][0]["display-temperature"],
        "65.3 °F"
    );
    let request = json!({"base_revision":server.state()["revision"],"layout":layout,"refresh_settings":{"quiet_hours":null}});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
    wait_idle(&server);
    let expected = STANDARD
        .decode(preview["png_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(server.png(), expected);
    server.stop();
    let restarted = Server::start_options(&path, None, Some(&bindings), Some(1700000000));
    assert_eq!(restarted.png(), expected);
    assert_eq!(source.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
#[ignore = "scripts/check.sh supplies the compiled guest"]
fn provider_cooldowns_survive_service_restart_and_empty_data_is_a_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let source = fetching_support::Observation::start();
    let bindings = fetching_support::registration(dir.path(), &source.url);
    let mut server = Server::start_options(&path, None, Some(&bindings), Some(1700000000));
    temperature_connection(&server, "home", 300);
    let _ = server
        .client
        .post(format!("{}/api/connections/home/fetch", server.url))
        .send()
        .unwrap();
    wait_connection(&server, "home", |connection| {
        connection["fetch"]["status"] == "ready"
    });
    server.stop();
    *source.reply.lock().unwrap() = (429, Some(120), json!({}));
    server = Server::start_options(&path, None, Some(&bindings), Some(1700000030));
    assert_eq!(
        server
            .client
            .post(format!("{}/api/connections/home/fetch", server.url))
            .send()
            .unwrap()
            .status(),
        202
    );
    wait_connection(&server, "home", |connection| {
        connection["fetch"]["status"] == "error"
    });
    temperature_connection(&server, "work", 300);
    let response = server
        .client
        .post(format!("{}/api/connections/work/fetch", server.url))
        .send()
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.headers()["retry-after"], "300");
    server.stop();
    server = Server::start_options(&path, None, Some(&bindings), Some(1700000031));
    let response = server
        .client
        .post(format!("{}/api/connections/home/fetch", server.url))
        .send()
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.headers()["retry-after"], "299");
    assert_eq!(source.hits.load(std::sync::atomic::Ordering::SeqCst), 2);
    server.stop();
    *source.reply.lock().unwrap() = (200, None, json!({"temperature_c":null}));
    server = Server::start_options(&path, None, Some(&bindings), Some(1700000330));
    wait_connection(&server, "home", |connection| {
        connection["fetch"]["status"] == "empty"
    });
    let catalog: Value = server
        .client
        .get(format!("{}/api/catalog", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let home = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|dataset| dataset["connection_id"] == "home")
        .unwrap();
    assert_eq!(home["rows"], 0);
    assert_eq!(home["stale"], false);
}

#[test]
#[ignore = "scripts/check.sh supplies the compiled guest"]
fn automatic_compiled_polling_keeps_fetching_during_quiet_hours() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let source = fetching_support::Observation::start();
    let bindings = fetching_support::registration(dir.path(), &source.url);
    let definitions = dir.path().join("definitions.json");
    let mut catalog: Value =
        serde_json::from_str(include_str!("../../../examples/simulator/connectors.json")).unwrap();
    catalog["connectors"][1]["min_poll_secs"] = json!(1);
    catalog["connectors"][1]["default_poll_secs"] = json!(1);
    std::fs::write(&definitions, serde_json::to_vec(&catalog).unwrap()).unwrap();
    let server = Server::start_options(&path, Some(&definitions), Some(&bindings), None);
    let state = server.state();
    let now = state["clock_now"].as_u64().unwrap();
    let clock = paperphase_core::ClockSpec {
        timezone: "UTC".into(),
        content: paperphase_core::ClockContent::Time,
        hour_cycle: paperphase_core::HourCycle::TwentyFour,
        date_format: paperphase_core::DateFormat::Iso,
    };
    let mut layout = state["layout"].clone();
    layout["components"][0]["blocks"][0]["source"] = json!({"kind":"field","binding":{"connection_id":"home","dataset_id":"current-weather","field_id":"display-temperature"}});
    let request = json!({"base_revision":state["revision"],"layout":layout,"refresh_settings":{"quiet_hours":{"timezone":"UTC","start":clock.format(now).unwrap(),"end":clock.format(now+120).unwrap()}}});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
    let before = server.png();
    temperature_connection(&server, "home", 1);
    wait_connection(&server, "home", |connection| {
        connection["fetch"]["status"] == "ready"
    });
    source.reply.lock().unwrap().2 = json!({"temperature_c":27.0});
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let disk: Value = serde_json::from_slice(
            &std::fs::read(path.with_extension("connections.json")).unwrap(),
        )
        .unwrap();
        if disk["data"]["home"]["snapshot"]["dataset"]["rows"][0]["temperature"] == 27.0 {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(server.state()["phase"], "paused");
    assert_eq!(server.png(), before);
    assert!(source.hits.load(std::sync::atomic::Ordering::SeqCst) >= 2);
    let request = json!({"base_revision":server.state()["revision"],"layout":layout,"refresh_settings":{"quiet_hours":null}});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        200
    );
    wait_idle(&server);
    assert_ne!(server.png(), before);
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

#[test]
fn draft_save_conflict_and_restart_preserve_the_display_contract() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let mut server = Server::start(&path);
    let state = server.state();
    let disk = std::fs::read(&path).unwrap();
    let original = server.png();
    let mut layout = state["layout"].clone();
    layout["components"] = json!([]);
    let response = server
        .client
        .post(format!("{}/api/preview", server.url))
        .json(&layout)
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    let preview: Value = response.json().unwrap();
    let expected = STANDARD
        .decode(preview["png_base64"].as_str().unwrap())
        .unwrap();
    assert_ne!(expected, original);
    assert_eq!(server.png(), original);
    assert_eq!(std::fs::read(&path).unwrap(), disk);
    assert_eq!(server.state()["revision"], 0);
    let request = json!({"base_revision": 0, "layout": layout});
    let response = server
        .client
        .put(format!("{}/api/layout", server.url))
        .json(&request)
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().unwrap()["phase"], "refreshing");
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        409
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.state()["phase"] != "idle" {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(server.png(), expected);
    assert_eq!(server.state()["displayed_revision"], 1);
    server.stop();
    let restarted = Server::start(&path);
    assert_eq!(restarted.state()["layout"], layout);
    assert_eq!(restarted.state()["revision"], 1);
    assert_eq!(restarted.png(), expected);
}

#[test]
fn invalid_geometry_failed_storage_and_foreign_requests_leave_saved_state_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let server = Server::start(&path);
    let state = server.state();
    let disk = std::fs::read(&path).unwrap();
    let mut layout = state["layout"].clone();
    layout["components"][1]["rect"] = layout["components"][0]["rect"].clone();
    let request = json!({"base_revision": 0, "layout": layout});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        422
    );
    for (header, value) in [
        ("Origin", "https://unrelated.example"),
        ("Host", "unrelated.example"),
        ("Sec-Fetch-Site", "cross-site"),
    ] {
        assert_eq!(
            server
                .client
                .get(format!("{}/api/state", server.url))
                .header(header, value)
                .send()
                .unwrap()
                .status(),
            403
        );
    }
    assert_eq!(
        server
            .client
            .post(format!("{}/api/preview", server.url))
            .header("Content-Type", "application/json")
            .body(vec![b' '; 1024 * 1024 + 1])
            .send()
            .unwrap()
            .status(),
        413
    );
    assert_eq!(std::fs::read(&path).unwrap(), disk);
    // Simulate a filesystem failure at the destination without permission assumptions.
    let backup = dir.path().join("backup.json");
    std::fs::rename(&path, &backup).unwrap();
    std::fs::create_dir(&path).unwrap();
    let request = json!({"base_revision": 0, "layout": state["layout"]});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&request)
            .send()
            .unwrap()
            .status(),
        500
    );
    assert_eq!(server.state()["revision"], 0);
    assert_eq!(std::fs::read(&backup).unwrap(), disk);
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(backup, path).unwrap();
}

#[test]
fn concurrent_saves_have_one_winner_and_one_workspace_owner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let server = Server::start(&path);
    let state = server.state();
    let result = Command::new(env!("CARGO_BIN_EXE_paperphase"))
        .args(["serve", "--listen", "127.0.0.1:0", "--state-file"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("another service owns this workspace")
    );
    let mut workers = Vec::new();
    for _ in 0..2 {
        let client = server.client.clone();
        let url = format!("{}/api/layout", server.url);
        let layout = state["layout"].clone();
        workers.push(std::thread::spawn(move || {
            client
                .put(url)
                .json(&json!({"base_revision": 0, "layout": layout}))
                .send()
                .unwrap()
                .status()
                .as_u16()
        }));
    }
    let mut statuses: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    assert_eq!(server.state()["revision"], 1);
}

#[test]
fn clock_and_quiet_settings_preserve_older_workspaces_and_validate_before_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.json");
    let layout: Value =
        serde_json::from_str(include_str!("../../../examples/simulator/layout.json")).unwrap();
    // The previous workspace schema has no refresh_settings field.
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({"schema_version":1,"revision":7,"layout":layout})).unwrap(),
    )
    .unwrap();
    let mut server = Server::start(&path);
    assert_eq!(
        server.state()["refresh_settings"],
        json!({"quiet_hours":null})
    );
    let zones: Vec<String> = server
        .client
        .get(format!("{}/api/timezones", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(zones.contains(&"Europe/Dublin".into()));
    let disk = std::fs::read(&path).unwrap();
    let mut layout = layout;
    layout["components"][0]["blocks"][0]["source"] =
        json!({"kind":"clock","clock":{"timezone":"Invalid/Zone"}});
    assert_eq!(
        server
            .client
            .post(format!("{}/api/preview", server.url))
            .json(&layout)
            .send()
            .unwrap()
            .status(),
        422
    );
    layout["components"][0]["blocks"][0]["source"]["clock"]["timezone"] = json!("Europe/Dublin");
    let invalid = json!({"base_revision":7,"layout":layout,"refresh_settings":{"quiet_hours":{"timezone":"Europe/Dublin","start":"22:00","end":"22:00"}}});
    assert_eq!(
        server
            .client
            .put(format!("{}/api/layout", server.url))
            .json(&invalid)
            .send()
            .unwrap()
            .status(),
        422
    );
    assert_eq!(std::fs::read(&path).unwrap(), disk);
    let request = json!({"base_revision":7,"layout":layout,"refresh_settings":{"quiet_hours":{"timezone":"Europe/Dublin","start":"22:00","end":"07:00"}}});
    let original = server.png();
    let response: Value = server
        .client
        .put(format!("{}/api/layout", server.url))
        .json(&request)
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(response["phase"], "paused");
    assert_eq!(server.png(), original);
    let response: Value = server
        .client
        .post(format!("{}/api/refresh", server.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(response["phase"], "paused");
    assert_eq!(server.png(), original);
    server.stop();
    let restarted = Server::start(&path);
    assert_eq!(restarted.state()["revision"], 8);
    assert_eq!(
        restarted.state()["refresh_settings"],
        request["refresh_settings"]
    );
}
