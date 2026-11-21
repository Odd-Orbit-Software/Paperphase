//! Independent supervisor: the app cannot replace its own recovery process.
use super::{
    package::{self, Target},
    store::{self, Active, Installation, Job, Phase, Slot},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Deserialize, Serialize)]
struct Process {
    pid: u32,
    stamp: String,
}
fn process_stamp(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = &stat[stat.rfind(')')? + 1..];
    let fields = tail.split_whitespace().collect::<Vec<_>>();
    if fields.first() == Some(&"Z") {
        return None;
    }
    fields.get(19).map(|s| s.to_string())
}
fn reap_orphan(config: &Installation) -> Result<()> {
    let path = config.root.join("child.json");
    if !path.exists() {
        return Ok(());
    }
    let process: Process = store::read(&path)?;
    if process.pid != std::process::id()
        && process_stamp(process.pid).as_ref() == Some(&process.stamp)
    {
        let _ = Command::new("kill")
            .args(["-TERM", &process.pid.to_string()])
            .status();
        let until = Instant::now() + Duration::from_secs(100);
        while process_stamp(process.pid).as_ref() == Some(&process.stamp) && Instant::now() < until
        {
            std::thread::sleep(Duration::from_millis(100));
        }
        if process_stamp(process.pid).as_ref() == Some(&process.stamp) {
            let _ = Command::new("kill")
                .args(["-KILL", &process.pid.to_string()])
                .status();
        }
    }
    Ok(())
}
/// Managed application children stop if their exact supervisor process disappears.
/// The persistent PID/start-time record also lets a new agent reap an orphan safely.
pub fn watch_parent() {
    let Ok(pid) = std::env::var("PAPERPHASE_SUPERVISOR_PID") else {
        return;
    };
    let Ok(pid) = pid.parse::<u32>() else {
        return;
    };
    let Ok(stamp) = std::env::var("PAPERPHASE_SUPERVISOR_STAMP") else {
        return;
    };
    std::thread::spawn(move || {
        loop {
            if process_stamp(pid).as_ref() != Some(&stamp) {
                let _ = Command::new("kill")
                    .args(["-TERM", &std::process::id().to_string()])
                    .status();
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

pub fn platform() -> String {
    format!("{}-unknown-linux-gnu", std::env::consts::ARCH)
}
fn capabilities(executable: &Path) -> Result<()> {
    let mut child = Command::new(executable)
        .arg("update-capabilities")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_remove("PAPERPHASE_INSTALLATION")
        .env_remove("PAPERPHASE_UPDATE_HOLD")
        .env_remove("PAPERPHASE_SUPERVISOR_PID")
        .env_remove("PAPERPHASE_SUPERVISOR_STAMP")
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .context("application capability output unavailable")?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(8193).read_to_end(&mut bytes).map(|_| bytes);
        let _ = send.send(result);
    });
    let until = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            bail!("application capability check timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    ensure!(
        status.success(),
        "application does not support the required update protocol"
    );
    let bytes = receive
        .recv_timeout(Duration::from_secs(1))
        .context("application capability output incomplete")??;
    ensure!(
        bytes.len() <= 8192,
        "application capability output exceeds limit"
    );
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(
        value["update_protocol"] == 1
            && value["state_schema"] == 1
            && value["platform"] == platform()
            && value["plugin_abi"] == package::PLUGIN_ABI,
        "application updater/state/plugin compatibility differs from this installation"
    );
    Ok(())
}
pub fn validate_payload(manifest: &package::Manifest, path: &Path) -> Result<()> {
    match &manifest.target {
        Target::Application { platform: target } => {
            ensure!(
                target == &platform(),
                "application package is for a different platform"
            );
            let mut header = [0; 20];
            File::open(path.join("paperphase"))?.read_exact(&mut header)?;
            ensure!(
                &header[..4] == b"\x7fELF" && header[4] == 2 && header[5] == 1,
                "application must be a native 64-bit little-endian ELF executable"
            );
            let machine = u16::from_le_bytes([header[18], header[19]]);
            ensure!(
                matches!(
                    (std::env::consts::ARCH, machine),
                    ("aarch64", 183) | ("x86_64", 62)
                ),
                "application ELF architecture differs from this host"
            );
            capabilities(&path.join("paperphase"))?;
        }
        Target::Plugin { connector_id, .. } => {
            let runtime =
                paperphase_runtime::PluginRuntime::new(paperphase_runtime::Limits::default())
                    .map_err(|e| anyhow::anyhow!("{e:#}"))?;
            let module = runtime
                .prepare(&fs::read(path.join("plugin.wasm"))?)
                .map_err(|e| anyhow::anyhow!("{e:#}"))?;
            runtime
                .validate_for_installation(&module)
                .map_err(|e| anyhow::anyhow!("{e:#}"))?;
            if path.join("icons.json").exists() {
                paperphase_render::IconStore::load_pack(&path.join("icons.json"), connector_id)?;
            }
        }
    }
    Ok(())
}
pub fn prepare(config: &Installation, job: &mut Job) -> Result<Slot> {
    let target = config
        .targets
        .get(&job.target)
        .context("unknown update target")?;
    let temp = tempfile::Builder::new()
        .prefix("verified-")
        .tempdir_in(config.root.join("slots"))?;
    let manifest = package::extract(
        &config.job_dir(&job.id)?.join("package.ppkg"),
        temp.path(),
        &target.trusted_keys,
        &target.repository,
        job.manifest.as_ref().map(|m| m.release.as_str()),
        &target.target,
    )?;
    validate_payload(&manifest, temp.path())?;
    let path = config.root.join("slots").join(&job.id);
    ensure!(!path.exists(), "update slot already exists");
    fs::rename(temp.path(), &path)?;
    File::open(config.root.join("slots"))?.sync_all()?;
    job.manifest = Some(manifest.clone());
    Ok(Slot {
        directory: path,
        manifest,
    })
}

/// Copy private data with bounded memory, regular files only, and a complete marker.
fn copy_tree(source: &Path, destination: &Path, budget: &mut u64) -> Result<()> {
    fs::create_dir(destination)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(destination, fs::Permissions::from_mode(0o700))?;
    }
    for entry in source.read_dir()? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let next = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &next, budget)?;
        } else {
            ensure!(
                kind.is_file(),
                "private state backup does not follow symlinks or special files"
            );
            let bytes = entry.metadata()?.len();
            *budget = budget
                .checked_sub(bytes)
                .context("private state exceeds 1 GiB backup budget")?;
            let input = File::open(entry.path())?;
            let mut input = input.take(bytes + 1);
            let mut output = File::create(&next)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                output.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            let copied = std::io::copy(&mut input, &mut output)?;
            ensure!(copied == bytes, "private state changed while backing up");
            output.flush()?;
            output.sync_all()?;
        }
    }
    File::open(destination)?.sync_all()?;
    Ok(())
}
fn backup(config: &Installation, job: &Job) -> Result<()> {
    let root = config.root.join("backups").join(&job.id);
    fs::create_dir(&root)?;
    copy_tree(
        &config.data_dir,
        &root.join("data"),
        &mut (1024 * 1024 * 1024),
    )?;
    store::persist(&root.join("complete.json"), &true)?;
    Ok(())
}
fn restore(config: &Installation, job: &Job) -> Result<()> {
    let root = config.root.join("backups").join(&job.id);
    let complete = root.join("complete.json");
    if !complete.exists() {
        // Backup never completed: no new app was launched, so retain original data.
        ensure!(!job.backup_complete, "complete state backup is missing");
        return Ok(());
    }
    ensure!(
        store::read::<bool>(&complete)?,
        "invalid backup completion marker"
    );
    let parent = config
        .data_dir
        .parent()
        .context("private state needs a parent directory")?;
    let temp = tempfile::Builder::new()
        .prefix(".paperphase-restore-")
        .tempdir_in(parent)?;
    let staged = temp.path().join("data");
    copy_tree(&root.join("data"), &staged, &mut (1024 * 1024 * 1024))?;
    if config.data_dir.exists() {
        // Retain failed-version data privately for operator recovery; never merge it.
        let failed = tempfile::Builder::new()
            .prefix(".paperphase-failed-")
            .tempdir_in(parent)?;
        let failed_path = failed.keep();
        fs::remove_dir(&failed_path)?;
        fs::rename(&config.data_dir, failed_path)?;
    }
    // A failed new version must not make rollback forget a recent panel operation.
    // Keep a conservative cooldown even if that version damaged its journal.
    preserve_display_cooldown(&staged)?;
    fs::rename(staged, &config.data_dir)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn preserve_display_cooldown(path: &Path) -> Result<()> {
    for entry in path.read_dir()? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            preserve_display_cooldown(&entry.path())?;
        } else if entry
            .file_name()
            .to_string_lossy()
            .ends_with(".display.json")
        {
            let mut value: serde_json::Value = store::read(&entry.path())?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let previous = value["not_before"]
                .as_u64()
                .context("invalid physical cooldown in backup")?;
            value["not_before"] = serde_json::json!(previous.max(now.saturating_add(180)));
            store::persist(&entry.path(), &value)?;
        }
    }
    Ok(())
}
fn stop(child: &mut Option<Child>) -> Result<()> {
    if let Some(mut process) = child.take() {
        if process.try_wait()?.is_none() {
            #[cfg(unix)]
            {
                let _ = Command::new("kill")
                    .args(["-TERM", &process.id().to_string()])
                    .status();
            }
            #[cfg(not(unix))]
            {
                let _ = process.kill();
            }
            let until = Instant::now() + Duration::from_secs(100);
            while process.try_wait()?.is_none() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(100));
            }
            if process.try_wait()?.is_none() {
                process.kill()?;
            }
        }
        process.wait()?;
    }
    Ok(())
}
fn launch(
    config: &Installation,
    path: &Path,
    active: &Active,
    hold: Option<&str>,
) -> Result<Child> {
    let executable = config.executable(active)?;
    let mut command = Command::new(&executable);
    if let Some(directory) = &config.working_dir {
        command.current_dir(directory);
    }
    command
        .args(&config.app_args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command.env("PAPERPHASE_INSTALLATION", path);
    command
        .env("PAPERPHASE_SUPERVISOR_PID", std::process::id().to_string())
        .env(
            "PAPERPHASE_SUPERVISOR_STAMP",
            process_stamp(std::process::id()).context("supervisor process identity unavailable")?,
        );
    if let Some(id) = hold {
        command
            .env("PAPERPHASE_UPDATE_HOLD", id)
            .env("PAPERPHASE_LAUNCH_ID", id);
    } else {
        command
            .env_remove("PAPERPHASE_UPDATE_HOLD")
            .env_remove("PAPERPHASE_LAUNCH_ID");
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("launching {}", executable.display()))?;
    let Some(stamp) = process_stamp(child.id()) else {
        let _ = child.kill();
        let _ = child.wait();
        bail!("application exited before startup");
    };
    let record = Process {
        pid: child.id(),
        stamp,
    };
    if let Err(error) = store::persist(&config.root.join("child.json"), &record) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}
fn health(
    config: &Installation,
    job: &Job,
    child: &mut Child,
    stopping: &AtomicBool,
) -> Result<()> {
    let client = paperphase_runtime::shared_http_client(Duration::from_secs(2))
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut ready_since = None;
    while Instant::now() < deadline {
        ensure!(
            !stopping.load(Ordering::SeqCst),
            "update supervisor interrupted"
        );
        ensure!(
            child.try_wait()?.is_none(),
            "updated application exited during health checking"
        );
        let ready = client
            .get(&config.health_url)
            .send()
            .ok()
            .filter(|r| r.status().is_success())
            .and_then(|r| {
                let mut bytes = Vec::new();
                r.take(8193).read_to_end(&mut bytes).ok()?;
                if bytes.len() > 8192 {
                    return None;
                }
                serde_json::from_slice::<serde_json::Value>(&bytes).ok()
            })
            .is_some_and(|v| {
                v["ready"] == true && v["launch_id"] == job.id && v["pid"] == child.id()
            });
        if ready {
            let since = ready_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_secs(5) {
                return Ok(());
            }
        } else {
            ready_since = None;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    bail!("application health check timed out")
}
fn phase(config: &Installation, job: &mut Job, next: Phase, message: &str) -> Result<()> {
    job.phase = next;
    job.message = message.into();
    config.save_job(job)
}
fn rollback(
    config: &Installation,
    path: &Path,
    job: &mut Job,
    child: &mut Option<Child>,
    stopping: &AtomicBool,
) -> Result<()> {
    phase(
        config,
        job,
        Phase::RollingBack,
        "The update did not start successfully. Restoring the previous version…",
    )?;
    stop(child)?;
    restore(config, job)?;
    let previous = job
        .previous
        .as_ref()
        .context("previous installation record is missing")?;
    config.save_active(previous)?;
    *child = Some(launch(config, path, previous, Some(&job.id))?);
    health(config, job, child.as_mut().unwrap(), stopping)?;
    phase(
        config,
        job,
        Phase::RolledBack,
        "The update failed. Your previous version and saved data have been restored.",
    )
}
fn activate(
    config: &Installation,
    path: &Path,
    job: &mut Job,
    child: &mut Option<Child>,
    stopping: &AtomicBool,
) -> Result<()> {
    ensure!(
        job.display_ack.is_some(),
        "Updating display acknowledgement is missing"
    );
    let slot = prepare(config, job)?;
    let mut active = config.active()?;
    job.previous = Some(active.clone());
    phase(config, job, Phase::BackingUp, "Backing up your saved data…")?;
    stop(child)?;
    backup(config, job)?;
    job.backup_complete = true;
    phase(
        config,
        job,
        Phase::Switching,
        "Activating the verified update…",
    )?;
    active.generation = active
        .generation
        .checked_add(1)
        .context("installation generation exhausted")?;
    active.slots.insert(job.target.clone(), slot);
    config.save_active(&active)?;
    phase(
        config,
        job,
        Phase::Starting,
        "Restarting Paperphase and checking it is ready…",
    )?;
    *child = Some(launch(config, path, &active, Some(&job.id))?);
    health(config, job, child.as_mut().unwrap(), stopping)?;
    phase(
        config,
        job,
        Phase::Healthy,
        "Update complete. Your display will resume according to its saved settings.",
    )
}

struct ChildOwner(Option<Child>);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = stop(&mut self.0);
    }
}

pub fn run(path: &Path, stopping: Arc<AtomicBool>) -> Result<()> {
    let config = Installation::load(path)?;
    config.init()?;
    let _lock = store::agent_lock(&config)?;
    reap_orphan(&config)?;
    let mut owner = ChildOwner(None);
    let child = &mut owner.0;
    if let Some(mut job) = config.job()? {
        if job.phase.handed_off() && job.phase != Phase::ReadyForActivation {
            if let Err(error) = rollback(&config, path, &mut job, child, &stopping) {
                if stopping.load(Ordering::SeqCst) {
                    stop(child)?;
                    return Ok(());
                }
                eprintln!("update recovery failed: {error:#}");
                if stopping.load(Ordering::SeqCst) {
                    return Ok(());
                }
                phase(
                    &config,
                    &mut job,
                    Phase::RecoveryRequired,
                    "Automatic recovery could not complete. Use the documented SSH recovery command.",
                )?;
                stop(child)?;
            }
        } else if !job.phase.terminal() && job.phase != Phase::ReadyForActivation {
            phase(
                &config,
                &mut job,
                Phase::Failed,
                "The previous download or display preparation was interrupted. Your installed version is unchanged; try the update again.",
            )?;
        }
    }
    let mut restarts = 0u32;
    while !stopping.load(Ordering::SeqCst) {
        let job = config.job()?;
        if let Some(mut job) = job.clone()
            && job.phase == Phase::ReadyForActivation
        {
            let result = activate(&config, path, &mut job, child, &stopping);
            if let Err(error) = result {
                eprintln!("update activation failed: {error:#}");
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                if job.previous.is_some() {
                    if let Err(error) = rollback(&config, path, &mut job, child, &stopping) {
                        eprintln!("update rollback failed: {error:#}");
                        phase(
                            &config,
                            &mut job,
                            Phase::RecoveryRequired,
                            "Automatic rollback failed. Your backup is retained; use SSH recovery.",
                        )?;
                        stop(child)?;
                    }
                } else {
                    phase(
                        &config,
                        &mut job,
                        Phase::Failed,
                        "The package failed final verification. Your installed version is unchanged.",
                    )?;
                }
            }
            continue;
        }
        if child
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().flatten().is_some())
        {
            *child = None;
            restarts = restarts.saturating_add(1);
        }
        if child.is_none() && job.is_none_or(|j| j.phase != Phase::RecoveryRequired) {
            if restarts > 0 {
                std::thread::sleep(Duration::from_secs(u64::from(restarts).min(30)));
            }
            *child = Some(launch(&config, path, &config.active()?, None)?);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    stop(child)
}

/// Explicit SSH recovery; requires the managed service to be stopped first.
pub fn recover(path: &Path) -> Result<()> {
    let config = Installation::load(path)?;
    config.init()?;
    let _lock = store::agent_lock(&config)?;
    reap_orphan(&config)?;
    let mut job = config
        .job()?
        .context("no update transaction is available for recovery")?;
    ensure!(
        job.previous.is_some(),
        "this transaction did not replace an installed version"
    );
    phase(
        &config,
        &mut job,
        Phase::RollingBack,
        "Restoring previous software and saved data through SSH recovery…",
    )?;
    restore(&config, &job)?;
    config.save_active(job.previous.as_ref().unwrap())?;
    phase(
        &config,
        &mut job,
        Phase::RolledBack,
        "Previous software and saved data restored. Start the managed service to resume.",
    )
}
