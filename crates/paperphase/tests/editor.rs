use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::blocking::Client;
use serde_json::{Value, json};

struct Server {
    child: Child,
    url: String,
    client: Client,
}
impl Server {
    fn start(state: &std::path::Path) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut child = Command::new(env!("CARGO_BIN_EXE_paperphase"))
            .args([
                "serve",
                "--listen",
                "127.0.0.1:0",
                "--now",
                "1700000000",
                "--state-file",
            ])
            .arg(state)
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
