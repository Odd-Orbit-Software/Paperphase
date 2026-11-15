//! Hardware execution, ownership and a private last-completed-frame journal.
use super::{Mode, Operation, Profile};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use clap::ValueEnum;
use paperphase_core::Layout;
use paperphase_render::Frame;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const WORKER: &str = include_str!("waveshare_v2.py");
const INTERVAL: u64 = 180;
const WATCHDOG: u64 = 80;
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(crate) enum Kind {
    #[default]
    Simulator,
    WaveshareV2,
    WaveshareV2Partial,
}
#[derive(Clone, Debug)]
pub(crate) struct Options {
    pub kind: Kind,
    pub vendor_dir: Option<PathBuf>,
    pub python: PathBuf,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            kind: Kind::Simulator,
            vendor_dir: None,
            python: "/usr/bin/python3".into(),
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Completed {
    revision: u64,
    layout: Layout,
    bits: String,
    completed_at: u64,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    not_before: u64,
    completed: Option<Completed>,
}
struct Physical {
    partial: bool,
    vendor: PathBuf,
    python: PathBuf,
    worker: tempfile::NamedTempFile,
    journal: Mutex<Journal>,
    file: PathBuf,
    lock: File,
}
pub(crate) struct Driver {
    physical: Option<Physical>,
}
pub(crate) struct Restored {
    pub revision: u64,
    pub layout: Layout,
    pub frame: Frame,
}
#[derive(Debug)]
pub(crate) struct Outcome {
    pub warning: Option<String>,
}
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn hash(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
impl Driver {
    pub fn simulator() -> Self {
        Self { physical: None }
    }
    pub fn open(options: Options, layout: &Layout, file: PathBuf) -> Result<Self> {
        if matches!(options.kind, Kind::Simulator) {
            ensure!(
                options.vendor_dir.is_none(),
                "vendor directory requires --display waveshare-v2"
            );
            return Ok(Self::simulator());
        }
        ensure!(
            cfg!(target_os = "linux"),
            "the Waveshare backend requires Linux"
        );
        ensure!(
            layout.display.width == 800 && layout.display.height == 480,
            "Waveshare V2 requires an 800x480 workspace"
        );
        let vendor = options
            .vendor_dir
            .context("--waveshare-vendor-dir is required")?
            .canonicalize()?;
        for (name, expected) in [
            (
                "__init__.py",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "epdconfig.py",
                "e53e361f5061b3a62ca143001e20afc530593035556addd57ecd4dbf815ec9f3",
            ),
            (
                "epd7in5_V2.py",
                "fafc41091c47d2ea367c3e85d84dca437e42370451cae5f5424b72b4614b55f2",
            ),
        ] {
            let source = std::fs::read(vendor.join("waveshare_epd").join(name))
                .with_context(|| format!("reading pinned vendor file {name}"))?;
            ensure!(
                hash(&source) == expected,
                "vendor file {name} differs from pinned Waveshare reference"
            );
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open("/tmp/paperphase-waveshare-v2.lock")?;
        lock.try_lock()
            .context("another process owns the physical display")?;
        let journal: Journal = if file.try_exists()? {
            crate::simulator::read_json(&file)?
        } else {
            Journal {
                schema_version: 1,
                ..Journal::default()
            }
        };
        ensure!(
            journal.schema_version == 1,
            "unsupported physical-display journal"
        );
        if let Some(completed) = &journal.completed {
            completed.layout.validate()?;
            ensure!(
                completed.layout.display.id == layout.display.id
                    && completed.layout.display.width == layout.display.width
                    && completed.layout.display.height == layout.display.height,
                "display journal identity differs from workspace"
            );
            Frame::from_packed(&layout.display, STANDARD.decode(&completed.bits)?)?;
        }
        let mut worker = tempfile::NamedTempFile::new()?;
        worker.write_all(WORKER.as_bytes())?;
        worker.flush()?;
        Ok(Self {
            physical: Some(Physical {
                partial: matches!(options.kind, Kind::WaveshareV2Partial),
                vendor,
                python: options.python,
                worker,
                journal: Mutex::new(journal),
                file,
                lock,
            }),
        })
    }
    pub fn is_physical(&self) -> bool {
        self.physical.is_some()
    }
    pub fn profile(&self) -> Profile {
        if let Some(physical) = &self.physical {
            Profile {
                id: if physical.partial {
                    "waveshare-7.5-v2-partial"
                } else {
                    "waveshare-7.5-v2-full"
                },
                partial: physical.partial,
                align_x: 8,
                align_y: 1,
                min_interval_ms: INTERVAL * 1000,
                max_partials: if physical.partial { 3 } else { 0 },
                max_full_age_secs: if physical.partial { 1800 } else { 7200 },
                max_partial_area_percent: if physical.partial { 40 } else { 0 },
                min_retry_ms: INTERVAL * 1000,
                scheduled_full: true,
            }
        } else {
            Profile::default()
        }
    }
    pub fn restore(&self) -> Result<Option<Restored>> {
        let Some(physical) = &self.physical else {
            return Ok(None);
        };
        let journal = physical
            .journal
            .lock()
            .map_err(|_| anyhow::anyhow!("display journal lock poisoned"))?;
        journal
            .completed
            .as_ref()
            .map(|completed| {
                Ok(Restored {
                    revision: completed.revision,
                    layout: completed.layout.clone(),
                    frame: Frame::from_packed(
                        &completed.layout.display,
                        STANDARD.decode(&completed.bits)?,
                    )?,
                })
            })
            .transpose()
    }
    pub fn restart_wait(&self) -> Duration {
        self.physical.as_ref().map_or(Duration::ZERO, |physical| {
            physical
                .journal
                .lock()
                .map_or(Duration::from_secs(INTERVAL), |journal| {
                    Duration::from_secs(journal.not_before.saturating_sub(unix_now()))
                })
        })
    }
    pub fn execute(
        &self,
        frame: &Frame,
        layout: &Layout,
        operation: &Operation,
    ) -> Result<Outcome> {
        let Some(physical) = &self.physical else {
            std::thread::sleep(Duration::from_millis(180));
            return Ok(Outcome { warning: None });
        };
        ensure!(
            operation.plan.mode == Mode::Full || physical.partial,
            "partial refresh is not validated for this profile"
        );
        let mut journal = physical
            .journal
            .lock()
            .map_err(|_| anyhow::anyhow!("display journal lock poisoned"))?;
        ensure!(
            unix_now() >= journal.not_before,
            "physical refresh interval has not elapsed"
        );
        // Commit a conservative restart deadline BEFORE energising the panel.
        let mut attempt = journal.clone();
        attempt.not_before = unix_now().saturating_add(INTERVAL + WATCHDOG);
        crate::server::persist(&physical.file, &attempt)
            .context("saving display attempt before hardware access")?;
        *journal = attempt;
        let result = physical.transfer(frame, operation, journal.completed.as_ref());
        journal.not_before = unix_now().saturating_add(INTERVAL);
        if result.is_ok() {
            journal.completed = Some(Completed {
                revision: operation.revision,
                layout: layout.clone(),
                bits: STANDARD.encode(frame.bits()),
                completed_at: unix_now(),
            });
        }
        let stored = crate::server::persist(&physical.file, &*journal);
        result?;
        let warning = stored.err().map(|error| {
            eprintln!("display completed, but recovery journal failed: {error:#}");
            "Screen updated, but its recovery record could not be saved. Check available storage."
                .to_owned()
        });
        Ok(Outcome { warning })
    }
}
impl Physical {
    fn launch(
        &self,
        pbm: Option<&Path>,
        partial: Option<(&Path, paperphase_core::Rect)>,
        budget: Duration,
    ) -> Result<serde_json::Value> {
        let mut output = tempfile::tempfile()?;
        // lgpio creates notification files in CWD; never use the read-only app/vendor tree.
        let runtime_directory = tempfile::tempdir()?;
        let mut command = Command::new(&self.python);
        command
            .current_dir(runtime_directory.path())
            .arg("-I")
            .arg("-B")
            .arg("-u")
            .arg(self.worker.path())
            .arg("--vendor-dir")
            .arg(&self.vendor)
            .arg("--parent")
            .arg(std::process::id().to_string());
        if let Some(path) = pbm {
            command.arg("--pbm").arg(path);
        } else {
            command.arg("--power-off-only");
        }
        if let Some((target, region)) = partial {
            command
                .arg("--partial-only")
                .arg("--partial-after")
                .arg(target)
                .arg("--region")
                .args(
                    [region.x, region.y, region.width, region.height]
                        .map(|value| value.to_string()),
                );
        }
        let mut child = command
            .stdin(Stdio::from(self.lock.try_clone()?))
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::inherit())
            .spawn()
            .context("starting Waveshare adapter")?;
        let started = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() >= budget {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("Waveshare adapter exceeded its watchdog");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        output.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        output.take(32769).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 32768, "oversized display adapter report");
        let report: serde_json::Value =
            serde_json::from_slice(&bytes).context("invalid display adapter report")?;
        ensure!(
            status.success()
                && report["ok"] == true
                && (pbm.is_none() || report["sleep_completed"] == true)
                && report["power_released"] == true,
            "physical display failed: {}",
            report
                .get("error")
                .and_then(|value| value.as_str())
                .unwrap_or("incomplete adapter acknowledgement")
        );
        Ok(report)
    }
    fn transfer(
        &self,
        frame: &Frame,
        operation: &Operation,
        previous: Option<&Completed>,
    ) -> Result<()> {
        let mut input = tempfile::NamedTempFile::new()?;
        frame.write_pbm(&mut input)?;
        input.flush()?;
        let mut baseline = None;
        let partial = if operation.plan.mode == Mode::Partial {
            let previous = previous.context("partial output requires a completed reference")?;
            let mut file = tempfile::NamedTempFile::new()?;
            Frame::from_packed(&previous.layout.display, STANDARD.decode(&previous.bits)?)?
                .write_pbm(&mut file)?;
            file.flush()?;
            baseline = Some(file);
            Some((input.path(), operation.plan.region))
        } else {
            None
        };
        let before = baseline.as_ref().map_or(input.path(), |file| file.path());
        match self.launch(Some(before), partial, Duration::from_secs(WATCHDOG)) {
            Ok(report) => {
                let (mode, acknowledgement, timing) = if partial.is_some() {
                    ensure!(
                        report["mode"] == "partial",
                        "partial acknowledgement mode differs"
                    );
                    ensure!(
                        report["frame_sha256"]
                            == hash(
                                &STANDARD
                                    .decode(&previous.context("partial reference missing")?.bits)?
                            ),
                        "partial acknowledgement reference differs"
                    );
                    let region = operation.plan.region;
                    ensure!(
                        report["region"]
                            == serde_json::json!([region.x, region.y, region.width, region.height])
                            && report["partial_payload_bytes"]
                                == u64::from(region.width) * u64::from(region.height) / 8,
                        "partial acknowledgement rectangle differs"
                    );
                    (
                        "partial",
                        &report["target_sha256"],
                        &report["partial_refresh_ms"],
                    )
                } else {
                    ensure!(
                        report["mode"] == "full",
                        "full acknowledgement mode differs"
                    );
                    ("full", &report["frame_sha256"], &report["refresh_ms"])
                };
                ensure!(
                    acknowledgement == &hash(frame.bits()),
                    "display acknowledgement frame differs"
                );
                eprintln!(
                    "physical display {mode} refresh: {timing} ms; operation {} ms",
                    report["elapsed_ms"]
                );
                Ok(())
            }
            Err(error) => {
                let _ = self.launch(None, None, Duration::from_secs(10));
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{Controller, Reason};
    fn scene(text: &str) -> Layout {
        serde_json::from_value(serde_json::json!({"schema_version":1,"display":{"id":"panel","width":800,"height":480},"components":[{"id":"note","rect":{"x":0,"y":0,"width":800,"height":480},"padding":4,"blocks":[{"id":"text","font_size":14,"source":{"kind":"literal","text":text}}]}]})).unwrap()
    }
    fn pixels(layout: &Layout) -> Frame {
        paperphase_render::Renderer::new()
            .unwrap()
            .render(
                layout,
                &paperphase_core::SnapshotSet {
                    schema_version: 1,
                    snapshots: vec![],
                },
                0,
            )
            .unwrap()
            .frame
    }
    fn operation() -> Operation {
        Operation {
            revision: 7,
            plan: super::super::Plan {
                mode: Mode::Full,
                reason: Reason::UnknownBaseline,
                region: paperphase_core::Rect {
                    x: 0,
                    y: 0,
                    width: 800,
                    height: 480,
                },
                dirty: None,
            },
        }
    }
    const ACK: &str = r#"import sys,json,hashlib,pathlib,tempfile
assert pathlib.Path.cwd().parent==pathlib.Path(tempfile.gettempdir())
pathlib.Path('.gpio-notify').write_text('private runtime file')
args=sys.argv
if '--power-off-only' in args:
 print(json.dumps({'ok':True,'power_released':True}));sys.exit(0)
p=pathlib.Path(args[args.index('--pbm')+1]);raw=p.read_bytes().split(b'\n',2)[2]
folder=pathlib.Path(args[args.index('--vendor-dir')+1]);(folder/'called').write_text('called')
print(json.dumps({'ok':True,'sleep_completed':True,'power_released':True,'mode':'full','frame_sha256':hashlib.sha256(raw).hexdigest(),'refresh_ms':1,'elapsed_ms':2}))
"#;
    const PARTIAL_ACK: &str = r#"import sys,json,hashlib,pathlib
args=sys.argv
assert '--partial-only' in args
before=pathlib.Path(args[args.index('--pbm')+1]).read_bytes()[11:]
after=pathlib.Path(args[args.index('--partial-after')+1]).read_bytes()[11:]
offset=args.index('--region')+1
region=list(map(int,args[offset:offset+4]))
assert len(before)==len(after)==48000 and before!=after
print(json.dumps({'ok':True,'sleep_completed':True,'power_released':True,'mode':'partial','frame_sha256':hashlib.sha256(before).hexdigest(),'target_sha256':hashlib.sha256(after).hexdigest(),'region':region,'partial_payload_bytes':region[2]*region[3]//8,'partial_refresh_ms':1,'elapsed_ms':2}))
"#;
    fn driver(dir: &Path, script: &str) -> Driver {
        let mut worker = tempfile::NamedTempFile::new().unwrap();
        worker.write_all(script.as_bytes()).unwrap();
        worker.flush().unwrap();
        Driver {
            physical: Some(Physical {
                partial: false,
                vendor: dir.to_path_buf(),
                python: "/usr/bin/python3".into(),
                worker,
                journal: Mutex::new(Journal {
                    schema_version: 1,
                    ..Journal::default()
                }),
                file: dir.join("panel.display.json"),
                lock: tempfile::tempfile().unwrap(),
            }),
        }
    }
    #[test]
    fn acknowledged_frame_is_retained_and_restart_spacing_is_durable() {
        let dir = tempfile::tempdir().unwrap();
        let driver = driver(dir.path(), ACK);
        let scene = scene("Physical output");
        let frame = pixels(&scene);
        driver.execute(&frame, &scene, &operation()).unwrap();
        let restored = driver.restore().unwrap().unwrap();
        assert_eq!(restored.frame, frame);
        assert_eq!(restored.revision, 7);
        let physical = driver.physical.as_ref().unwrap();
        let persisted: Journal = crate::simulator::read_json(&physical.file).unwrap();
        assert!(persisted.not_before >= unix_now() + 178);
        assert!(driver.restart_wait() >= Duration::from_secs(178));
        assert!(
            driver
                .execute(&frame, &scene, &operation())
                .unwrap_err()
                .to_string()
                .contains("interval")
        );
        let encoded = STANDARD.decode(&persisted.completed.unwrap().bits).unwrap();
        assert_eq!(encoded, frame.bits());
    }
    #[test]
    fn failed_worker_preserves_the_last_completed_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let mut driver = driver(dir.path(), ACK);
        let scene = scene("Before failure");
        let frame = pixels(&scene);
        driver.execute(&frame, &scene, &operation()).unwrap();
        let physical = driver.physical.as_mut().unwrap();
        physical.journal.lock().unwrap().not_before = 0;
        physical.worker.as_file_mut().set_len(0).unwrap();
        physical.worker.seek(SeekFrom::Start(0)).unwrap();
        physical
            .worker
            .write_all(
                b"import json\nprint(json.dumps({'ok':False,'error':'Fixture driver failure'}))\n",
            )
            .unwrap();
        physical.worker.flush().unwrap();
        assert!(
            driver
                .execute(&pixels(&scene), &scene, &operation())
                .is_err()
        );
        assert_eq!(driver.restore().unwrap().unwrap().frame, frame);
        let persisted: Journal =
            crate::simulator::read_json(&driver.physical.as_ref().unwrap().file).unwrap();
        assert_eq!(persisted.completed.unwrap().revision, 7);
        assert!(persisted.not_before >= unix_now() + 178);
    }
    #[test]
    fn failed_attempt_storage_prevents_hardware_access() {
        let dir = tempfile::tempdir().unwrap();
        let driver = driver(dir.path(), ACK);
        std::fs::create_dir(&driver.physical.as_ref().unwrap().file).unwrap();
        let scene = scene("Never sent");
        assert!(
            driver
                .execute(&pixels(&scene), &scene, &operation())
                .is_err()
        );
        assert!(!dir.path().join("called").exists());
        assert!(driver.restore().unwrap().is_none());
    }
    #[test]
    fn wrong_frame_acknowledgement_cannot_publish_a_completed_image() {
        let dir = tempfile::tempdir().unwrap();
        let driver = driver(
            dir.path(),
            &ACK.replace("hashlib.sha256(raw).hexdigest()", "'wrong-frame'"),
        );
        let scene = scene("Must match");
        assert!(
            driver
                .execute(&pixels(&scene), &scene, &operation())
                .is_err()
        );
        assert!(driver.restore().unwrap().is_none());
    }
    #[test]
    fn receipt_failure_after_panel_success_remains_a_success_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let script=ACK.replace("print(json.dumps({'ok':True,'sleep_completed'", "(folder/'panel.display.json').unlink();(folder/'panel.display.json').mkdir()\nprint(json.dumps({'ok':True,'sleep_completed'");
        let driver = driver(dir.path(), &script);
        let scene = scene("Physically completed");
        let frame = pixels(&scene);
        let outcome = driver.execute(&frame, &scene, &operation()).unwrap();
        assert!(outcome.warning.is_some());
        assert_eq!(driver.restore().unwrap().unwrap().frame, frame);
    }
    #[test]
    fn watchdog_reaps_a_stalled_worker() {
        let dir = tempfile::tempdir().unwrap();
        let driver = driver(dir.path(), "import time\ntime.sleep(20)\n");
        let started = std::time::Instant::now();
        let result =
            driver
                .physical
                .as_ref()
                .unwrap()
                .launch(None, None, Duration::from_millis(150));
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn hardware_profile_forces_full_refresh_and_bounds_retry_rate() {
        let dir = tempfile::tempdir().unwrap();
        let driver = driver(dir.path(), ACK);
        let mut controller = Controller::new();
        controller.profile = driver.profile();
        controller.baseline_known = false;
        let scene = scene("First frame");
        let frame = pixels(&scene);
        let now = tokio::time::Instant::now();
        let plan = controller
            .plan(
                &frame,
                &frame,
                &scene,
                &scene,
                &paperphase_core::RefreshPolicy::default(),
                now,
            )
            .unwrap()
            .unwrap();
        assert_eq!(plan.mode, Mode::Full);
        assert_eq!(plan.reason, Reason::UnknownBaseline);
        controller.failed(now);
        assert!(
            controller.ready_at(&paperphase_core::RefreshPolicy::default(), now)
                >= now + Duration::from_secs(180)
        );
        assert!(!controller.profile.partial);
    }

    fn prepare_partial(dir: &Path, script: &str) -> (Driver, Layout, Frame, Frame, Operation) {
        let mut driver = driver(dir, ACK);
        let old = scene("Before");
        let before = pixels(&old);
        driver.execute(&before, &old, &operation()).unwrap();
        let physical = driver.physical.as_mut().unwrap();
        physical.partial = true;
        physical.journal.lock().unwrap().not_before = 0;
        physical.worker.as_file_mut().set_len(0).unwrap();
        physical.worker.seek(SeekFrom::Start(0)).unwrap();
        physical.worker.write_all(script.as_bytes()).unwrap();
        physical.worker.flush().unwrap();
        let new = scene("After");
        let after = pixels(&new);
        let mut controller = Controller::new();
        controller.profile = driver.profile();
        controller.baseline_known = true;
        let plan = controller
            .plan(
                &before,
                &after,
                &old,
                &new,
                &paperphase_core::RefreshPolicy::default(),
                tokio::time::Instant::now(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(plan.mode, Mode::Partial);
        (driver, new, before, after, Operation { revision: 8, plan })
    }

    #[test]
    fn partial_dispatch_acknowledges_both_frames_and_exact_region_before_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let (driver, layout, _, desired, operation) = prepare_partial(dir.path(), PARTIAL_ACK);
        driver.execute(&desired, &layout, &operation).unwrap();
        let restored = driver.restore().unwrap().unwrap();
        assert_eq!(restored.frame, desired);
        assert_eq!(restored.revision, 8);
        assert!(driver.restart_wait() >= Duration::from_secs(178));
        let profile = driver.profile();
        assert!(profile.partial);
        assert_eq!(profile.max_partials, 3);
        assert_eq!(profile.max_partial_area_percent, 40);
        assert_eq!(profile.min_interval_ms, 180000);
    }

    #[test]
    fn partial_acknowledgement_mismatches_retain_previous_receipt() {
        for (original, wrong) in [
            ("'region':region", "'region':[0,0,8,1]"),
            ("hashlib.sha256(before).hexdigest()", "'wrong-reference'"),
            ("hashlib.sha256(after).hexdigest()", "'wrong-target'"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (driver, layout, before, desired, operation) =
                prepare_partial(dir.path(), &PARTIAL_ACK.replace(original, wrong));
            assert!(driver.execute(&desired, &layout, &operation).is_err());
            assert_eq!(driver.restore().unwrap().unwrap().frame, before);
        }
    }
}
