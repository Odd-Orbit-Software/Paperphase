use base64::{Engine, engine::general_purpose::STANDARD};
use paperphase::update::{
    agent,
    package::{self, Manifest, Target},
    store::{self, Installation, InstalledTarget, Phase},
};
use ring::signature::KeyPair;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Managed {
    process: Child,
    directory: tempfile::TempDir,
    config: PathBuf,
    url: String,
    client: reqwest::blocking::Client,
    key: Vec<u8>,
}
impl Managed {
    fn start() -> Self {
        Self::start_options(false)
    }
    fn start_options(plugin: bool) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        fs::create_dir(&data).unwrap();
        fs::write(
            data.join("private-owner-note.txt"),
            b"preserve this private fixture",
        )
        .unwrap();
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let key = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .unwrap()
            .as_ref()
            .to_vec();
        let pair = ring::signature::Ed25519KeyPair::from_pkcs8(&key).unwrap();
        let config = directory.path().join("installation.json");
        let mut value = Installation {
            schema_version: 1,
            root: directory.path().join("installation"),
            data_dir: data.clone(),
            working_dir: None,
            status_port: {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                listener.local_addr().unwrap().port()
            },
            health_url: format!("http://127.0.0.1:{port}/api/update-health"),
            app_args: vec![
                "serve".into(),
                "--listen".into(),
                format!("127.0.0.1:{port}"),
                "--state-file".into(),
                data.join("workspace.json").to_string_lossy().into(),
                "--now".into(),
                "1700000000".into(),
            ],
            targets: std::collections::BTreeMap::from([(
                "application".into(),
                InstalledTarget {
                    label: "Paperphase".into(),
                    repository: "fixture/paperphase".into(),
                    trusted_keys: std::collections::BTreeMap::from([(
                        "ephemeral-test".into(),
                        STANDARD.encode(pair.public_key().as_ref()),
                    )]),
                    target: Target::Application {
                        platform: agent::platform(),
                    },
                    baseline: PathBuf::from(env!("CARGO_BIN_EXE_paperphase")),
                },
            )]),
        };
        if plugin {
            let baseline = directory.path().join("baseline.wasm");
            fs::write(&baseline, plugin_wasm(18)).unwrap();
            let mut target = value.targets["application"].clone();
            target.label = "Sample temperature plugin".into();
            target.target = Target::Plugin {
                connector_id: "example.temperature".into(),
                abi: package::PLUGIN_ABI.into(),
            };
            target.baseline = baseline.clone();
            value.targets.insert("example.temperature".into(), target);
            let bindings = data.join("bindings.json");
            store::persist(&bindings,&json!({"schema_version":1,"bindings":[{"connector_id":"example.temperature","wasm":baseline,"source_url":"http://127.0.0.1:9/unused","origin_interval_secs":1,"stale_after_secs":3600}]})).unwrap();
            value.app_args.truncate(value.app_args.len() - 2);
            value.app_args.extend([
                "--connector-bindings".into(),
                bindings.to_string_lossy().into(),
            ]);
        }
        store::persist(&config, &value).unwrap();
        let process = Self::spawn(&config);
        let result = Self {
            process,
            directory,
            config,
            url: format!("http://127.0.0.1:{port}"),
            client: reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            key,
        };
        result.wait(|s| s["managed"] == true);
        result
    }
    fn spawn(config: &Path) -> Child {
        Command::new(env!("CARGO_BIN_EXE_paperphase-update-agent"))
            .args(["run", "--installation"])
            .arg(config)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap()
    }
    fn installation(&self) -> Installation {
        Installation::load(&self.config).unwrap()
    }
    fn status(&self) -> Option<Value> {
        self.client
            .get(format!("{}/api/updates", self.url))
            .send()
            .ok()?
            .json()
            .ok()
    }
    fn wait(&self, condition: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if let Some(status) = self.status()
                && condition(&status)
            {
                return status;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!(
            "update condition timed out; last status: {:?}",
            self.status()
        );
    }
    fn bundle(&self, binary: &Path, build: &str) -> PathBuf {
        let source = self.directory.path().join(format!("source-{build}"));
        fs::create_dir(&source).unwrap();
        fs::copy(binary, source.join("paperphase")).unwrap();
        assert!(
            Command::new("strip")
                .arg("--strip-debug")
                .arg(source.join("paperphase"))
                .status()
                .unwrap()
                .success()
        );
        let bytes = fs::read(source.join("paperphase")).unwrap();
        let manifest = Manifest {
            schema_version: 1,
            state_schema: 1,
            update_protocol: 1,
            repository: "fixture/paperphase".into(),
            release: format!("v0.1.0-{build}"),
            version: "0.1.0".into(),
            build: build.into(),
            target: Target::Application {
                platform: agent::platform(),
            },
            files: vec![package::Entry {
                name: "paperphase".into(),
                bytes: bytes.len() as u64,
                sha256: package::sha256(&bytes),
            }],
        };
        let path = self.directory.path().join(format!("{build}.ppkg"));
        package::write(&manifest, "ephemeral-test", &self.key, &source, &path).unwrap();
        path
    }
    fn upload(&self, path: &Path) {
        let response = self
            .client
            .post(format!(
                "{}/api/updates/upload?target=application",
                self.url
            ))
            .header("content-type", "application/octet-stream")
            .body(fs::read(path).unwrap())
            .send()
            .unwrap();
        assert_eq!(response.status(), 202, "{}", response.text().unwrap());
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-TERM", &self.process.id().to_string()])
            .status();
        let until = Instant::now() + Duration::from_secs(8);
        while self.process.try_wait().unwrap().is_none() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[test]
fn managed_update_acknowledges_notice_then_activates_and_rolls_back_a_failed_native_build() {
    let managed = Managed::start();
    let data = managed.installation().data_dir;
    let before = fs::read(data.join("workspace.json")).unwrap();
    let good = managed.bundle(Path::new(env!("CARGO_BIN_EXE_paperphase")), "working");
    managed.upload(&good);
    let starting = managed.wait(|s| s["job"]["phase"] == "starting");
    let helper = format!(
        "http://127.0.0.1:{}/api/update-status",
        managed.installation().status_port
    );
    let progress = managed
        .client
        .get(&helper)
        .header("origin", &managed.url)
        .send()
        .unwrap();
    assert_eq!(progress.status(), 200);
    assert_eq!(
        progress.headers()["access-control-allow-origin"],
        managed.url
    );
    let progress: Value = progress.json().unwrap();
    assert_eq!(progress["job"]["id"], starting["job"]["id"]);
    assert!(progress["job"].get("previous").is_none());
    assert_eq!(
        managed
            .client
            .get(&helper)
            .header("origin", "http://foreign.invalid:8080")
            .send()
            .unwrap()
            .status(),
        403
    );
    assert_eq!(managed.client.post(&helper).send().unwrap().status(), 405);
    let state: Value = managed
        .client
        .get(format!("{}/api/state", managed.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(
        managed
            .client
            .put(format!("{}/api/layout", managed.url))
            .json(&json!({"base_revision":state["revision"],"layout":state["layout"]}))
            .send()
            .unwrap()
            .status(),
        423
    );
    let status = managed.wait(|s| s["job"]["phase"] == "healthy");
    assert_eq!(status["targets"][0]["installed"]["build"], "working");
    let job = managed.installation().job().unwrap().unwrap();
    assert!(job.display_ack.is_some());
    assert!(job.backup_complete);
    assert_eq!(
        fs::read(data.join("private-owner-note.txt")).unwrap(),
        b"preserve this private fixture"
    );
    assert_eq!(fs::read(data.join("workspace.json")).unwrap(), before);
    let failed = managed.directory.path().join("fails-startup");
    let source = managed.directory.path().join("fails-startup.rs");
    let capabilities=serde_json::to_string(&json!({"update_protocol":1,"state_schema":1,"platform":agent::platform(),"plugin_abi":package::PLUGIN_ABI})).unwrap();
    let program = format!(
        "fn main(){{if std::env::args().nth(1).as_deref()==Some(\"update-capabilities\"){{println!(\"{{}}\",{:?});}}else{{std::process::exit(1);}}}}",
        capabilities
    );
    fs::write(&source, program).unwrap();
    assert!(
        Command::new("rustc")
            .args(["--crate-name", "fails_startup"])
            .arg(&source)
            .arg("-o")
            .arg(&failed)
            .status()
            .unwrap()
            .success()
    );
    let bad = managed.bundle(&failed, "fails-startup");
    managed.upload(&bad);
    let status = managed.wait(|s| s["job"]["phase"] == "rolled_back");
    assert_eq!(status["targets"][0]["installed"]["build"], "working");
    assert_eq!(fs::read(data.join("workspace.json")).unwrap(), before);
    assert_eq!(
        fs::read(data.join("private-owner-note.txt")).unwrap(),
        b"preserve this private fixture"
    );
}
#[test]
fn rejected_packages_leave_the_normal_frame_and_installed_files_untouched() {
    let managed = Managed::start();
    let before = managed
        .client
        .get(format!("{}/api/display.png", managed.url))
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    let path = managed.directory.path().join("invalid.ppkg");
    fs::write(&path, b"not a signed package").unwrap();
    managed.upload(&path);
    let status = managed.wait(|s| s["job"]["phase"] == "failed");
    assert!(status["targets"][0]["installed"].is_null());
    assert!(
        managed
            .installation()
            .job()
            .unwrap()
            .unwrap()
            .display_ack
            .is_none()
    );
    assert_eq!(
        managed
            .client
            .get(format!("{}/api/display.png", managed.url))
            .send()
            .unwrap()
            .bytes()
            .unwrap(),
        before
    );
    let state: Value = managed
        .client
        .get(format!("{}/api/state", managed.url))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let rejected = managed
        .client
        .post(format!("{}/api/updates/install", managed.url))
        .header("origin", "https://foreign.invalid")
        .json(&json!({"target":"application","tag":"v0.1.0"}))
        .send()
        .unwrap();
    assert_eq!(rejected.status(), 403);
    assert_eq!(state["updating"], false);
}
#[test]
fn interrupted_activation_recovers_the_previous_version_and_private_state() {
    let mut managed = Managed::start();
    let before = fs::read(managed.installation().data_dir.join("workspace.json")).unwrap();
    let good = managed.bundle(Path::new(env!("CARGO_BIN_EXE_paperphase")), "interrupted");
    managed.upload(&good);
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if managed
            .installation()
            .job()
            .unwrap()
            .is_some_and(|job| job.phase == Phase::Starting)
        {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    managed.process.kill().unwrap();
    managed.process.wait().unwrap();
    managed.process = Managed::spawn(&managed.config);
    let status = managed.wait(|s| s["job"]["phase"] == "rolled_back");
    assert!(status["targets"][0]["installed"].is_null());
    assert_eq!(
        fs::read(managed.installation().data_dir.join("workspace.json")).unwrap(),
        before
    );
}

fn plugin_wasm(value: u32) -> Vec<u8> {
    let data=json!({"schema_version":1,"id":"temperature","fields":[{"id":"value","kind":"number"}],"rows":[{"value":value}]}).to_string();
    let escaped = data
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect::<String>();
    wat::parse_str(format!(r#"(module (import "paperphase_v0" "publish" (func $publish (param i32 i32) (result i32))) (memory (export "memory") 1) (data (i32.const 0) "{escaped}") (func (export "run") (result i32) i32.const 0 i32.const {} call $publish drop i32.const 0))"#,data.len())).unwrap()
}
#[test]
fn installed_plugin_updates_keep_host_grants_and_cached_data_then_run_the_new_wasm() {
    let managed = Managed::start_options(true);
    let config = managed.installation();
    let data = &config.data_dir;
    let response=managed.client.put(format!("{}/api/connections/sample",managed.url)).json(&json!({"base_revision":0,"connector_id":"example.temperature","label":"Sample sensor","enabled":true,"poll_interval_secs":300,"settings":{"location":"Test bench","units":"metric"}})).send().unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().unwrap());
    let cache = || -> Value {
        serde_json::from_slice(&fs::read(data.join("workspace.connections.json")).unwrap()).unwrap()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while cache()["data"]["sample"]["snapshot"]["dataset"]["rows"][0]["value"] != 18 {
        assert!(
            Instant::now() < deadline,
            "initial plugin did not publish: {}",
            cache()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let before = cache();
    let grants = fs::read(data.join("bindings.json")).unwrap();
    let source = managed.directory.path().join("new-plugin");
    fs::create_dir(&source).unwrap();
    let bytes = plugin_wasm(24);
    fs::write(source.join("plugin.wasm"), &bytes).unwrap();
    let manifest = Manifest {
        schema_version: 1,
        state_schema: 1,
        update_protocol: 1,
        repository: "fixture/paperphase".into(),
        release: "v0.1.0-plugin.2".into(),
        version: "0.1.0".into(),
        build: "new-plugin".into(),
        target: config.targets["example.temperature"].target.clone(),
        files: vec![package::Entry {
            name: "plugin.wasm".into(),
            bytes: bytes.len() as u64,
            sha256: package::sha256(&bytes),
        }],
    };
    let path = managed.directory.path().join("plugin.ppkg");
    package::write(&manifest, "ephemeral-test", &managed.key, &source, &path).unwrap();
    let response = managed
        .client
        .post(format!(
            "{}/api/updates/upload?target=example.temperature",
            managed.url
        ))
        .header("content-type", "application/octet-stream")
        .body(fs::read(path).unwrap())
        .send()
        .unwrap();
    assert_eq!(response.status(), 202);
    managed.wait(|s| s["job"]["phase"] == "healthy");
    assert_eq!(fs::read(data.join("bindings.json")).unwrap(), grants);
    assert_eq!(cache()["connections"], before["connections"]);
    assert_eq!(
        cache()["data"]["sample"]["snapshot"],
        before["data"]["sample"]["snapshot"]
    );
    let response = managed
        .client
        .post(format!("{}/api/connections/sample/fetch", managed.url))
        .send()
        .unwrap();
    assert_eq!(
        response.status(),
        429,
        "the plugin update must preserve its provider cooldown"
    );
    let retry: Value = response.json().unwrap();
    let seconds = retry["retry_after_secs"].as_u64().unwrap();
    assert!(seconds <= 30);
    std::thread::sleep(Duration::from_secs(seconds + 1));
    assert_eq!(
        managed
            .client
            .post(format!("{}/api/connections/sample/fetch", managed.url))
            .send()
            .unwrap()
            .status(),
        202
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while cache()["data"]["sample"]["snapshot"]["dataset"]["rows"][0]["value"] != 24 {
        assert!(
            Instant::now() < deadline,
            "updated plugin did not publish: {}",
            cache()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
