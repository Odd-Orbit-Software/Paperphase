//! A signed manifest followed by one gzip stream of ordered regular files.
//! No executable hooks, archive paths, native Wasm caches or symlinks are accepted.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, bufread::GzDecoder, write::GzEncoder};
use ring::{digest, signature};
use serde::{Deserialize, Serialize};

const MAGIC: &[u8] = b"PAPERPHASE-PKG1\n";
const HEADER_LIMIT: usize = 65536;
pub const DOWNLOAD_LIMIT: u64 = 64 * 1024 * 1024;
const EXPANDED_LIMIT: u64 = 128 * 1024 * 1024;
pub const PLUGIN_ABI: &str = "paperphase_v0";
pub type PublicKeys = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Application { platform: String },
    Plugin { connector_id: String, abi: String },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub repository: String,
    pub release: String,
    pub version: String,
    pub build: String,
    pub state_schema: u32,
    pub update_protocol: u32,
    pub target: Target,
    pub files: Vec<Entry>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    key_id: String,
    manifest: String,
    signature: String,
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, bytes).as_ref())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn repository_valid(repo: &str) -> bool {
    let parts: Vec<_> = repo.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 100
                && !matches!(*part, "." | "..")
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && self.state_schema == 1 && self.update_protocol == 1,
            "unsupported package or state schema"
        );
        ensure!(
            repository_valid(&self.repository),
            "invalid release repository"
        );
        for (label, value) in [
            ("release", &self.release),
            ("version", &self.version),
            ("build", &self.build),
        ] {
            ensure!(
                !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control),
                "invalid {label}"
            );
        }
        let allowed: BTreeSet<String> = match &self.target {
            Target::Application { platform } => {
                ensure!(
                    !platform.is_empty()
                        && platform.len() <= 80
                        && platform
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
                    "invalid application platform"
                );
                BTreeSet::from(["paperphase".into(), "NOTICE.txt".into()])
            }
            Target::Plugin { connector_id, abi } => {
                ensure!(
                    !connector_id.is_empty()
                        && connector_id.len() <= 128
                        && connector_id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
                    "invalid plugin identity"
                );
                ensure!(abi == PLUGIN_ABI, "unsupported plugin ABI");
                BTreeSet::from_iter(
                    [
                        "plugin.wasm",
                        "icons.json",
                        "icons-16.png",
                        "icons-24.png",
                        "icons-32.png",
                        "icons-48.png",
                        "icons-64.png",
                        "NOTICE.txt",
                    ]
                    .map(str::to_owned),
                )
            }
        };
        ensure!(
            !self.files.is_empty() && self.files.len() <= allowed.len(),
            "invalid package file count"
        );
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for entry in &self.files {
            ensure!(
                allowed.contains(&entry.name) && names.insert(entry.name.clone()),
                "unsupported or duplicate package filename"
            );
            ensure!(
                entry.bytes > 0 && entry.bytes <= EXPANDED_LIMIT,
                "invalid package file size"
            );
            total = total
                .checked_add(entry.bytes)
                .context("package size overflow")?;
            ensure!(
                total <= EXPANDED_LIMIT,
                "expanded package exceeds size limit"
            );
            ensure!(
                entry.sha256.len() == 64
                    && entry
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid file SHA-256"
            );
        }
        let primary = match self.target {
            Target::Application { .. } => "paperphase",
            Target::Plugin { .. } => "plugin.wasm",
        };
        ensure!(
            names.contains(primary),
            "package is missing its executable payload"
        );
        if matches!(self.target, Target::Plugin { .. }) {
            ensure!(
                self.files.iter().find(|e| e.name == primary).unwrap().bytes <= 8 * 1024 * 1024,
                "plugin module exceeds package limit"
            );
        }
        Ok(())
    }
}

/// Extract only into a new private directory owned by this operation.
/// Callers retain the directory only after all signature, compatibility and file checks pass.
pub fn extract(
    package: &Path,
    destination: &Path,
    keys: &PublicKeys,
    repository: &str,
    release: Option<&str>,
    target: &Target,
) -> Result<Manifest> {
    ensure!(
        package.metadata()?.len() <= DOWNLOAD_LIMIT,
        "package exceeds download limit"
    );
    ensure!(
        destination.is_dir() && destination.read_dir()?.next().is_none(),
        "package destination must be an empty private directory"
    );
    let mut input = BufReader::new(File::open(package)?);
    let mut magic = vec![0; MAGIC.len()];
    input.read_exact(&mut magic)?;
    ensure!(magic == MAGIC, "not a Paperphase release package");
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    ensure!(
        length > 0 && length <= HEADER_LIMIT,
        "package header exceeds limit"
    );
    let mut header = vec![0; length];
    input.read_exact(&mut header)?;
    let envelope: Envelope =
        serde_json::from_slice(&header).context("invalid signed package header")?;
    let key = keys
        .get(&envelope.key_id)
        .context("publisher is not trusted on this installation")?;
    let key = STANDARD
        .decode(key)
        .context("invalid trusted publisher key")?;
    let manifest = STANDARD
        .decode(&envelope.manifest)
        .context("invalid package manifest encoding")?;
    let signature = STANDARD
        .decode(&envelope.signature)
        .context("invalid package signature encoding")?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(&manifest, &signature)
        .map_err(|_| anyhow::anyhow!("publisher signature could not be verified"))?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest).context("invalid package manifest")?;
    manifest.validate()?;
    ensure!(
        manifest.repository == repository && release.is_none_or(|tag| tag == manifest.release),
        "package does not belong to the selected release"
    );
    ensure!(
        &manifest.target == target,
        "package is not compatible with the selected application or plugin"
    );
    let mut payload = GzDecoder::new(input);
    for entry in &manifest.files {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join(&entry.name))?;
        let mut remaining = entry.bytes;
        let mut hash = digest::Context::new(&digest::SHA256);
        let mut chunk = [0; 65536];
        while remaining > 0 {
            let length = remaining.min(chunk.len() as u64) as usize;
            payload
                .read_exact(&mut chunk[..length])
                .context("truncated package payload")?;
            hash.update(&chunk[..length]);
            file.write_all(&chunk[..length])?;
            remaining -= length as u64;
        }
        ensure!(
            hex(hash.finish().as_ref()) == entry.sha256,
            "package file digest mismatch: {}",
            entry.name
        );
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(
                if entry.name == "paperphase" {
                    0o755
                } else {
                    0o600
                },
            ))?;
        }
    }
    ensure!(
        payload.read(&mut [0])? == 0,
        "package contains undeclared payload"
    );
    let mut input = payload.into_inner();
    ensure!(
        input.fill_buf()?.is_empty(),
        "package contains trailing data"
    );
    Ok(manifest)
}

/// Publisher-side packaging. The private key is supplied explicitly; never part of a release.
pub fn write(
    manifest: &Manifest,
    key_id: &str,
    private_key: &[u8],
    source: &Path,
    output: &Path,
) -> Result<()> {
    manifest.validate()?;
    ensure!(
        !key_id.is_empty() && key_id.len() <= 64,
        "invalid publisher key identifier"
    );
    let key = signature::Ed25519KeyPair::from_pkcs8(private_key)
        .map_err(|_| anyhow::anyhow!("invalid Ed25519 signing key"))?;
    let bytes = serde_json::to_vec(manifest)?;
    let header = serde_json::to_vec(&Envelope {
        key_id: key_id.into(),
        manifest: STANDARD.encode(&bytes),
        signature: STANDARD.encode(key.sign(&bytes).as_ref()),
    })?;
    ensure!(header.len() <= HEADER_LIMIT, "package header exceeds limit");
    let mut file = tempfile::NamedTempFile::new_in(output.parent().unwrap_or(Path::new(".")))?;
    file.write_all(MAGIC)?;
    file.write_all(&(header.len() as u32).to_be_bytes())?;
    file.write_all(&header)?;
    let mut payload = GzEncoder::new(file, Compression::default());
    for entry in &manifest.files {
        let path = source.join(&entry.name);
        ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "package input cannot be a symlink"
        );
        let mut input = File::open(path)?;
        ensure!(
            input.metadata()?.is_file() && input.metadata()?.len() == entry.bytes,
            "package source size differs from manifest"
        );
        let mut hash = digest::Context::new(&digest::SHA256);
        let mut chunk = [0; 65536];
        let mut remaining = entry.bytes;
        while remaining > 0 {
            let length = remaining.min(chunk.len() as u64) as usize;
            input.read_exact(&mut chunk[..length])?;
            hash.update(&chunk[..length]);
            payload.write_all(&chunk[..length])?;
            remaining -= length as u64;
        }
        ensure!(
            input.read(&mut [0])? == 0 && hex(hash.finish().as_ref()) == entry.sha256,
            "package source changed or digest differs from manifest"
        );
    }
    let file = payload.finish()?;
    file.as_file().sync_all()?;
    ensure!(
        file.as_file().metadata()?.len() <= DOWNLOAD_LIMIT,
        "compressed package exceeds limit"
    );
    file.persist(output).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::{rand::SystemRandom, signature::KeyPair};

    fn setup() -> (tempfile::TempDir, Vec<u8>, PublicKeys, Manifest) {
        let dir = tempfile::tempdir().unwrap();
        let private = signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .unwrap()
            .as_ref()
            .to_vec();
        let key = signature::Ed25519KeyPair::from_pkcs8(&private).unwrap();
        let keys = BTreeMap::from([(
            "test-only".into(),
            STANDARD.encode(key.public_key().as_ref()),
        )]);
        let data = b"\0asm\x01\0\0\0";
        std::fs::write(dir.path().join("plugin.wasm"), data).unwrap();
        let manifest = Manifest {
            schema_version: 1,
            repository: "example/plugins".into(),
            release: "v0.1.0-test".into(),
            version: "0.1.0".into(),
            build: "test-build".into(),
            state_schema: 1,
            update_protocol: 1,
            target: Target::Plugin {
                connector_id: "demo.weather".into(),
                abi: PLUGIN_ABI.into(),
            },
            files: vec![Entry {
                name: "plugin.wasm".into(),
                bytes: data.len() as u64,
                sha256: sha256(data),
            }],
        };
        (dir, private, keys, manifest)
    }
    #[test]
    fn signed_packages_roundtrip_and_bind_repository_release_target_and_publisher() {
        let (dir, key, keys, manifest) = setup();
        let path = dir.path().join("test.ppkg");
        write(&manifest, "test-only", &key, dir.path(), &path).unwrap();
        let output = tempfile::tempdir().unwrap();
        let decoded = extract(
            &path,
            output.path(),
            &keys,
            "example/plugins",
            Some("v0.1.0-test"),
            &manifest.target,
        )
        .unwrap();
        assert_eq!(decoded.build, "test-build");
        assert_eq!(
            std::fs::read(output.path().join("plugin.wasm")).unwrap(),
            b"\0asm\x01\0\0\0"
        );
        for (repo, tag, target, trust) in [
            ("other/repo", None, manifest.target.clone(), keys.clone()),
            (
                "example/plugins",
                Some("different"),
                manifest.target.clone(),
                keys.clone(),
            ),
            (
                "example/plugins",
                None,
                Target::Plugin {
                    connector_id: "another.plugin".into(),
                    abi: PLUGIN_ABI.into(),
                },
                keys.clone(),
            ),
            (
                "example/plugins",
                None,
                manifest.target.clone(),
                BTreeMap::new(),
            ),
        ] {
            let output = tempfile::tempdir().unwrap();
            assert!(extract(&path, output.path(), &trust, repo, tag, &target).is_err());
            assert!(output.path().read_dir().unwrap().next().is_none());
        }
    }
    #[test]
    fn tampering_and_undeclared_bytes_do_not_produce_an_activatable_package() {
        let (dir, key, keys, manifest) = setup();
        let path = dir.path().join("test.ppkg");
        write(&manifest, "test-only", &key, dir.path(), &path).unwrap();
        let original = std::fs::read(&path).unwrap();
        // Alter a signed manifest byte while preserving a valid JSON envelope.
        let start = MAGIC.len() + 4;
        let size = u32::from_be_bytes(original[MAGIC.len()..start].try_into().unwrap()) as usize;
        let mut envelope: Envelope =
            serde_json::from_slice(&original[start..start + size]).unwrap();
        let mut altered = manifest.clone();
        altered.build = "forged".into();
        envelope.manifest = STANDARD.encode(serde_json::to_vec(&altered).unwrap());
        let header = serde_json::to_vec(&envelope).unwrap();
        let mut bad = MAGIC.to_vec();
        bad.extend((header.len() as u32).to_be_bytes());
        bad.extend(header);
        bad.extend(&original[start + size..]);
        std::fs::write(&path, bad).unwrap();
        let output = tempfile::tempdir().unwrap();
        assert!(
            extract(
                &path,
                output.path(),
                &keys,
                &manifest.repository,
                None,
                &manifest.target
            )
            .unwrap_err()
            .to_string()
            .contains("signature")
        );
        let mut trailing = original.clone();
        trailing.extend(b"undeclared");
        std::fs::write(&path, trailing).unwrap();
        let output = tempfile::tempdir().unwrap();
        assert!(
            extract(
                &path,
                output.path(),
                &keys,
                &manifest.repository,
                None,
                &manifest.target
            )
            .is_err()
        );
        let mut corrupt = original;
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        std::fs::write(&path, corrupt).unwrap();
        let output = tempfile::tempdir().unwrap();
        assert!(
            extract(
                &path,
                output.path(),
                &keys,
                &manifest.repository,
                None,
                &manifest.target
            )
            .is_err()
        );
    }
    #[test]
    fn manifests_bound_expansion_and_cannot_name_paths_caches_or_duplicate_files() {
        let (_, _, _, manifest) = setup();
        for name in [
            "../outside",
            "/absolute",
            "dir/plugin.wasm",
            "plugin.cwasm",
            "hooks.sh",
            "..\\outside",
        ] {
            let mut invalid = manifest.clone();
            invalid.files[0].name = name.into();
            assert!(invalid.validate().is_err());
        }
        let mut invalid = manifest.clone();
        invalid.files.push(invalid.files[0].clone());
        assert!(invalid.validate().is_err());
        let mut invalid = manifest.clone();
        invalid.files[0].bytes = EXPANDED_LIMIT + 1;
        assert!(invalid.validate().is_err());
        let mut invalid = manifest.clone();
        invalid.state_schema = 2;
        assert!(invalid.validate().is_err());
        let mut invalid = manifest;
        invalid.target = Target::Plugin {
            connector_id: "demo.weather".into(),
            abi: "another_abi".into(),
        };
        assert!(invalid.validate().is_err());
    }
}
