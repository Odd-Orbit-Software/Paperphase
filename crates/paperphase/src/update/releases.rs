//! GitHub discovery/downloads never reuse connector authentication or accept arbitrary origins.
use super::{
    package::{self, Target},
    store::InstalledTarget,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};
use url::Url;

#[derive(Clone, Deserialize, Serialize)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub url: String,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub asset: Asset,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Asset {
    pub name: String,
    pub bytes: u64,
    pub digest: Option<String>,
}
#[derive(Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}
#[derive(Clone, Deserialize)]
struct GithubAsset {
    name: String,
    size: u64,
    digest: Option<String>,
}

pub fn asset_name(target: &Target) -> String {
    match target {
        Target::Application { platform } => format!("paperphase-{platform}.ppkg"),
        Target::Plugin { connector_id, .. } => format!("{connector_id}.ppkg"),
    }
}
fn decode_segment(value: &str) -> Result<String> {
    let mut out = Vec::new();
    let mut chars = value.bytes();
    while let Some(c) = chars.next() {
        if c == b'%' {
            let a = chars.next().context("incomplete URL escape")?;
            let b = chars.next().context("incomplete URL escape")?;
            out.push(
                (a as char).to_digit(16).context("invalid URL escape")? as u8 * 16
                    + (b as char).to_digit(16).context("invalid URL escape")? as u8,
            );
        } else {
            out.push(c);
        }
    }
    Ok(String::from_utf8(out)?)
}
pub fn release_tag(value: &str, repository: &str) -> Result<String> {
    let url = Url::parse(value).context("use a full HTTPS GitHub release URL")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "use a GitHub release URL without credentials, query or fragment"
    );
    let parts: Vec<_> = url
        .path_segments()
        .context("invalid release URL")?
        .collect();
    ensure!(
        parts.len() >= 5 && parts[2] == "releases" && parts[3] == "tag",
        "use a specific GitHub release page"
    );
    ensure!(
        format!("{}/{}", parts[0], parts[1]).eq_ignore_ascii_case(repository),
        "this release belongs to a different configured repository"
    );
    let tag = decode_segment(&parts[4..].join("/"))?;
    validate_tag(&tag)?;
    Ok(tag)
}
fn validate_tag(tag: &str) -> Result<()> {
    ensure!(
        !tag.is_empty() && tag.len() <= 128 && !tag.chars().any(char::is_control),
        "invalid release tag"
    );
    Ok(())
}
fn api_url(repo: &str, tag: Option<&str>) -> Result<Url> {
    ensure!(package::repository_valid(repo), "invalid GitHub repository");
    let mut url = Url::parse(&format!("https://api.github.com/repos/{repo}/releases"))?;
    if let Some(tag) = tag {
        validate_tag(tag)?;
        url.path_segments_mut().unwrap().extend(["tags", tag]);
    } else {
        url.set_query(Some("per_page=20"));
    }
    Ok(url)
}
fn public_url(repo: &str, tag: &str) -> Result<Url> {
    let mut url = Url::parse(&format!("https://github.com/{repo}/releases/tag/"))?;
    url.path_segments_mut().unwrap().pop_if_empty().push(tag);
    Ok(url)
}
fn bounded_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::blocking::Response,
) -> Result<T> {
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("No published release was found in this repository.");
    }
    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        bail!("GitHub is limiting update checks. Try again later.");
    }
    ensure!(
        response.status().is_success(),
        "GitHub release check failed ({})",
        response.status()
    );
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "GitHub release metadata exceeds limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn select(
    raw: GithubRelease,
    target: &InstalledTarget,
    experimental: bool,
) -> Result<Option<Release>> {
    if raw.draft || raw.prerelease && !experimental {
        return Ok(None);
    }
    validate_tag(&raw.tag_name)?;
    let name = asset_name(&target.target);
    let Some(asset) = raw.assets.into_iter().find(|a| a.name == name) else {
        return Ok(None);
    };
    ensure!(
        asset.size > 0 && asset.size <= package::DOWNLOAD_LIMIT,
        "release package exceeds download limit"
    );
    if let Some(digest) = &asset.digest {
        let value = digest
            .strip_prefix("sha256:")
            .context("unsupported GitHub asset digest")?;
        ensure!(
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid GitHub asset digest"
        );
    }
    let mut notes = raw.body.unwrap_or_default();
    if notes.len() > 16384 {
        let mut end = 16384;
        while !notes.is_char_boundary(end) {
            end -= 1;
        }
        notes.truncate(end);
    }
    Ok(Some(Release {
        url: public_url(&target.repository, &raw.tag_name)?.into(),
        name: raw
            .name
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| raw.tag_name.clone()),
        tag: raw.tag_name,
        notes,
        prerelease: raw.prerelease,
        published_at: raw.published_at,
        asset: Asset {
            name,
            bytes: asset.size,
            digest: asset.digest,
        },
    }))
}
pub fn check(
    target: &InstalledTarget,
    experimental: bool,
    tag: Option<&str>,
) -> Result<Vec<Release>> {
    let client = paperphase_runtime::shared_http_client(Duration::from_secs(20))
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let response = client
        .get(api_url(&target.repository, tag)?)
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .send()
        .context("Could not reach GitHub. Check the connection and try again.")?;
    let raw = if tag.is_some() {
        vec![bounded_json::<GithubRelease>(response)?]
    } else {
        bounded_json::<Vec<GithubRelease>>(response)?
    };
    ensure!(raw.len() <= 20, "too many release records");
    let mut releases = Vec::new();
    for item in raw {
        if let Some(item) = select(item, target, experimental)? {
            releases.push(item);
        }
    }
    Ok(releases)
}
fn download_origin(url: &Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}
pub fn download(
    target: &InstalledTarget,
    release: &Release,
    destination: &Path,
    mut progress: impl FnMut(u64, u64) -> Result<()>,
) -> Result<()> {
    let client = paperphase_runtime::shared_http_client(Duration::from_secs(120))
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let mut url = Url::parse(&format!(
        "https://github.com/{}/releases/download/",
        target.repository
    ))?;
    url.path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .extend([release.tag.as_str(), release.asset.name.as_str()]);
    let mut final_response = None;
    for _ in 0..5 {
        ensure!(
            download_origin(&url),
            "release download redirected outside supported GitHub hosts"
        );
        let response = client
            .get(url.clone())
            .send()
            .context("Could not download the release package.")?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get("location")
                .context("download redirect is missing its destination")?
                .to_str()?;
            url = url.join(location)?;
            continue;
        }
        ensure!(
            response.status().is_success(),
            "Release download failed ({}).",
            response.status()
        );
        final_response = Some(response);
        break;
    }
    let mut response = final_response.context("too many release download redirects")?;
    ensure!(
        response
            .content_length()
            .is_none_or(|n| n == release.asset.bytes),
        "release download size changed"
    );
    let mut file = File::create(destination)?;
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut total = 0u64;
    let mut chunk = [0; 65536];
    let mut last = Instant::now();
    loop {
        let n = response.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .context("download size overflow")?;
        ensure!(
            total <= release.asset.bytes && total <= package::DOWNLOAD_LIMIT,
            "release download exceeds its declared size"
        );
        file.write_all(&chunk[..n])?;
        hash.update(&chunk[..n]);
        if last.elapsed() >= Duration::from_secs(1) {
            progress(total, release.asset.bytes)?;
            last = Instant::now();
        }
    }
    ensure!(
        total == release.asset.bytes,
        "release download was incomplete"
    );
    if let Some(expected) = &release.asset.digest {
        let actual = hash
            .finish()
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        ensure!(
            expected == &format!("sha256:{actual}"),
            "release download checksum differs from GitHub metadata"
        );
    }
    file.sync_all()?;
    progress(total, total)?;
    Ok(())
}

#[derive(Serialize, Default)]
pub struct Discovery {
    pub targets: std::collections::BTreeMap<String, Vec<Release>>,
    pub errors: std::collections::BTreeMap<String, String>,
}
/// One release-list request per repository, even when all bundled plugins share it.
pub fn check_many(
    targets: &std::collections::BTreeMap<String, InstalledTarget>,
    experimental: bool,
) -> Result<Discovery> {
    let client = paperphase_runtime::shared_http_client(Duration::from_secs(20))
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    let mut repositories: std::collections::BTreeMap<String, Result<Vec<GithubRelease>, String>> =
        Default::default();
    let mut discovery = Discovery::default();
    for (id, target) in targets {
        if !repositories.contains_key(&target.repository) {
            let result = (|| -> Result<Vec<GithubRelease>> {
                let response = client
                    .get(api_url(&target.repository, None)?)
                    .header("accept", "application/vnd.github+json")
                    .header("x-github-api-version", "2022-11-28")
                    .send()
                    .context(
                        "Could not reach GitHub. Try again when the connection is available.",
                    )?;
                let values: Vec<GithubRelease> = bounded_json(response)?;
                ensure!(values.len() <= 20, "too many release records");
                Ok(values)
            })();
            repositories.insert(target.repository.clone(), result.map_err(|e| e.to_string()));
        }
        match &repositories[&target.repository] {
            Err(error) => {
                discovery.errors.insert(id.clone(), error.clone());
            }
            Ok(raw) => {
                let result = raw
                    .iter()
                    .cloned()
                    .filter_map(|raw| select(raw, target, experimental).transpose())
                    .collect::<Result<Vec<_>>>();
                match result {
                    Ok(releases) => {
                        discovery.targets.insert(id.clone(), releases);
                    }
                    Err(error) => {
                        discovery.errors.insert(id.clone(), error.to_string());
                    }
                }
            }
        }
    }
    Ok(discovery)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target() -> InstalledTarget {
        InstalledTarget {
            label: "Sample plugin".into(),
            repository: "owner/plugins".into(),
            trusted_keys: Default::default(),
            target: Target::Plugin {
                connector_id: "sample.plugin".into(),
                abi: package::PLUGIN_ABI.into(),
            },
            baseline: "/tmp/local.wasm".into(),
        }
    }
    #[test]
    fn release_urls_and_download_origins_cannot_escape_the_registered_publisher() {
        assert_eq!(
            release_tag(
                "https://github.com/owner/plugins/releases/tag/feature%2Fv0.1.0+test",
                "owner/plugins"
            )
            .unwrap(),
            "feature/v0.1.0+test"
        );
        for value in [
            "http://github.com/owner/plugins/releases/tag/v1",
            "https://github.com/other/plugins/releases/tag/v1",
            "https://github.com/owner/plugins/releases/latest",
            "https://github.com@localhost/owner/plugins/releases/tag/v1",
            "https://github.com/owner/plugins/releases/tag/v1?token=anything",
        ] {
            assert!(release_tag(value, "owner/plugins").is_err());
        }
        for value in [
            "http://github.com/file",
            "https://127.0.0.1/file",
            "https://github.com.attacker.invalid/file",
            "https://github.com:8443/file",
            "https://user:pass@github.com/file",
        ] {
            assert!(!download_origin(&Url::parse(value).unwrap()));
        }
        assert!(download_origin(
            &Url::parse("https://release-assets.githubusercontent.com/file?temporary=download")
                .unwrap()
        ));
    }
    #[test]
    fn experimental_releases_are_opt_in_and_notes_truncate_at_a_unicode_boundary() {
        let target = target();
        let raw = GithubRelease {
            tag_name: "v0.1.0-experimental".into(),
            name: Some("Sample release".into()),
            body: Some("界".repeat(6000)),
            draft: false,
            prerelease: true,
            published_at: None,
            assets: vec![GithubAsset {
                name: asset_name(&target.target),
                size: 1024,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
            }],
        };
        assert!(select(raw.clone(), &target, false).unwrap().is_none());
        let release = select(raw, &target, true).unwrap().unwrap();
        assert_eq!(release.notes.len(), 16383);
        assert!(release.prerelease);
    }
}
