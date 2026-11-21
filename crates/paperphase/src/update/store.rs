//! Private durable installation/transaction records shared by app and supervisor.
use super::package::{Manifest, PublicKeys, Target, repository_valid};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledTarget {
    pub label: String,
    pub repository: String,
    pub trusted_keys: PublicKeys,
    pub target: Target,
    /// Operator-installed executable/Wasm, used until the first managed update.
    pub baseline: PathBuf,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Installation {
    pub schema_version: u32,
    pub root: PathBuf,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    pub health_url: String,
    #[serde(default)]
    pub status_port: u16,
    pub app_args: Vec<String>,
    pub targets: BTreeMap<String, InstalledTarget>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Active {
    pub generation: u64,
    pub slots: BTreeMap<String, Slot>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub directory: PathBuf,
    pub manifest: Manifest,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Downloading,
    Verifying,
    WaitingForDisplay,
    ReadyForActivation,
    BackingUp,
    Switching,
    Starting,
    Healthy,
    RollingBack,
    RolledBack,
    Failed,
    RecoveryRequired,
}
impl Phase {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Healthy | Self::RolledBack | Self::Failed | Self::RecoveryRequired
        )
    }
    pub fn handed_off(self) -> bool {
        matches!(
            self,
            Self::ReadyForActivation
                | Self::BackingUp
                | Self::Switching
                | Self::Starting
                | Self::RollingBack
        )
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub schema_version: u32,
    pub id: String,
    pub target: String,
    pub phase: Phase,
    pub message: String,
    pub manifest: Option<Manifest>,
    pub previous: Option<Active>,
    #[serde(default)]
    pub backup_complete: bool,
    pub display_ack: Option<String>,
}
impl Installation {
    pub fn load(path: &Path) -> Result<Self> {
        let config: Self = read(path)?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported installation schema");
        ensure!(
            self.root.is_absolute()
                && self.data_dir.is_absolute()
                && !self.root.starts_with(&self.data_dir)
                && !self.data_dir.starts_with(&self.root),
            "installation and private data folders must be distinct absolute paths"
        );
        ensure!(
            self.working_dir
                .as_ref()
                .is_none_or(|path| path.is_absolute() && path.is_dir()),
            "working directory must be an existing absolute directory"
        );
        ensure!(
            self.app_args.len() <= 64 && self.app_args.first().is_some_and(|a| a == "serve"),
            "installation must launch the Paperphase serve command"
        );
        ensure!(
            !self.targets.is_empty() && self.targets.len() <= 33,
            "invalid installed-target count"
        );
        let health = url::Url::parse(&self.health_url)?;
        ensure!(
            health.scheme() == "http"
                && health.host_str() == Some("127.0.0.1")
                && health.username().is_empty()
                && health.password().is_none()
                && health.path() == "/api/update-health"
                && health.query().is_none()
                && health.fragment().is_none(),
            "health check must use the local Paperphase health endpoint"
        );
        let application = self
            .targets
            .get("application")
            .context("application target is required")?;
        ensure!(
            matches!(application.target, Target::Application { .. }),
            "invalid application target"
        );
        for (id, target) in &self.targets {
            ensure!(
                identifier(id)
                    && target.baseline.is_absolute()
                    && repository_valid(&target.repository)
                    && !target.label.is_empty()
                    && target.label.len() <= 128,
                "invalid installed target"
            );
            if let Target::Plugin { connector_id, abi } = &target.target {
                ensure!(
                    id == connector_id && abi == super::package::PLUGIN_ABI,
                    "plugin identity/ABI mismatch"
                );
            } else {
                ensure!(
                    id == "application",
                    "only one application target is supported"
                );
            }
            ensure!(target.trusted_keys.len() <= 16, "too many publisher keys");
            for (key, value) in &target.trusted_keys {
                use base64::Engine;
                ensure!(
                    identifier(key)
                        && base64::engine::general_purpose::STANDARD
                            .decode(value)?
                            .len()
                            == 32,
                    "invalid publisher public key"
                );
            }
        }
        Ok(())
    }
    pub fn init(&self) -> Result<()> {
        self.validate()?;
        for path in [
            &self.root,
            self.root.join("jobs").as_path(),
            self.root.join("slots").as_path(),
            self.root.join("backups").as_path(),
        ] {
            fs::create_dir_all(path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
        }
        ensure!(
            self.data_dir.is_dir()
                || self.job()?.is_some_and(|j| (j.phase.handed_off()
                    || j.phase == Phase::RecoveryRequired)
                    && j.backup_complete),
            "private data folder does not exist"
        );
        Ok(())
    }
    pub fn active(&self) -> Result<Active> {
        let path = self.root.join("active.json");
        if path.exists() {
            read(&path)
        } else {
            Ok(Active::default())
        }
    }
    pub fn save_active(&self, active: &Active) -> Result<()> {
        persist(&self.root.join("active.json"), active)
    }
    pub fn job_dir(&self, id: &str) -> Result<PathBuf> {
        ensure!(job_id(id), "invalid update transaction identifier");
        Ok(self.root.join("jobs").join(id))
    }
    pub fn job(&self) -> Result<Option<Job>> {
        let current = self.root.join("current.json");
        if !current.exists() {
            return Ok(None);
        }
        let id: String = read(&current)?;
        let job: Job = read(&self.job_dir(&id)?.join("job.json"))?;
        ensure!(
            job.schema_version == 1 && job.id == id && self.targets.contains_key(&job.target),
            "invalid update transaction"
        );
        Ok(Some(job))
    }
    pub fn save_job(&self, job: &Job) -> Result<()> {
        persist(&self.job_dir(&job.id)?.join("job.json"), job)
    }
    pub fn start_job(&self, target: &str) -> Result<Job> {
        ensure!(
            self.targets.contains_key(target),
            "unknown installed update target"
        );
        ensure!(
            self.job()?
                .is_none_or(|j| j.phase.terminal() && j.phase != Phase::RecoveryRequired),
            "another update is running or requires recovery"
        );
        let mut bytes = [0u8; 16];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
            .map_err(|_| anyhow::anyhow!("could not create update identifier"))?;
        let id = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let path = self.job_dir(&id)?;
        fs::create_dir(&path)?;
        let job = Job {
            schema_version: 1,
            id: id.clone(),
            target: target.into(),
            phase: Phase::Downloading,
            message: "Preparing your update…".into(),
            manifest: None,
            previous: None,
            backup_complete: false,
            display_ack: None,
        };
        self.save_job(&job)?;
        persist(&self.root.join("current.json"), &id)?;
        Ok(job)
    }
    pub fn executable(&self, active: &Active) -> Result<PathBuf> {
        if let Some(slot) = active.slots.get("application") {
            self.check_slot("application", slot)?;
            Ok(slot.directory.join("paperphase"))
        } else {
            Ok(self.targets["application"].baseline.clone())
        }
    }
    pub fn check_slot(&self, id: &str, slot: &Slot) -> Result<()> {
        let target = self.targets.get(id).context("unregistered update target")?;
        ensure!(
            slot.manifest.repository == target.repository && slot.manifest.target == target.target,
            "installed package identity differs from registration"
        );
        let parent = self.root.join("slots").canonicalize()?;
        ensure!(
            slot.directory.canonicalize()?.starts_with(&parent),
            "installed slot is outside the managed installation"
        );
        slot.manifest.validate()?;
        for entry in &slot.manifest.files {
            let path = slot.directory.join(&entry.name);
            ensure!(
                path.symlink_metadata()?.is_file() && path.metadata()?.len() == entry.bytes,
                "installed file is missing or changed"
            );
            let file = File::open(path)?;
            let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
            use std::io::Read;
            let mut reader = file.take(entry.bytes + 1);
            let mut bytes = [0; 65536];
            loop {
                let n = reader.read(&mut bytes)?;
                if n == 0 {
                    break;
                }
                hash.update(&bytes[..n]);
            }
            let actual = hash
                .finish()
                .as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            ensure!(
                actual == entry.sha256,
                "installed file checksum differs from verified package"
            );
        }
        Ok(())
    }
}
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && value != "."
        && value != ".."
}
pub fn job_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    use std::io::Read;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "update record exceeds limit");
    serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid update record: {}", path.display()))
}
pub fn persist(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    ensure!(bytes.len() <= 1024 * 1024, "update record exceeds limit");
    let parent = path
        .parent()
        .context("update record needs a parent directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    if let Err(error) = File::open(parent).and_then(|directory| directory.sync_all()) {
        eprintln!("update record committed; directory sync failed: {error}");
    }
    Ok(())
}
pub fn agent_lock(config: &Installation) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(config.root.join("agent.lock"))?;
    file.try_lock()
        .context("another update agent owns this installation")?;
    Ok(file)
}

pub fn public_job(job: Job) -> serde_json::Value {
    serde_json::json!({"id":job.id,"target":job.target,"phase":job.phase,"message":job.message,"release":job.manifest.map(|m|serde_json::json!({"tag":m.release,"version":m.version,"build":m.build})),"finished":job.phase.terminal()})
}
