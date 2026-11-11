use std::{
    fs::File,
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use paperphase_core::{Layout, SnapshotSet};
use paperphase_render::Renderer;
use serde::de::DeserializeOwned;
use serde_json::json;

pub(crate) const MAX_JSON_BYTES: usize = 1024 * 1024;

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .take(MAX_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_JSON_BYTES,
        "{} exceeds the 1 MiB input limit",
        path.display()
    );
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn path_identity(path: &Path) -> Result<PathBuf> {
    if path.try_exists()? {
        return path.canonicalize().context("resolving existing file");
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().context("file path requires a filename")?;
    Ok(parent
        .canonicalize()
        .context("resolving output directory")?
        .join(name))
}

pub(crate) fn same_file(a: &Path, b: &Path) -> Result<bool> {
    if path_identity(a)? == path_identity(b)? {
        return Ok(true);
    }
    #[cfg(unix)]
    if let (Ok(a), Ok(b)) = (a.metadata(), b.metadata()) {
        use std::os::unix::fs::MetadataExt;
        return Ok(a.dev() == b.dev() && a.ino() == b.ino());
    }
    Ok(false)
}

pub fn run(
    layout: &Path,
    data: Option<&Path>,
    output: &Path,
    pbm: Option<&Path>,
    now: Option<u64>,
) -> Result<()> {
    if let Some(path) = pbm {
        ensure!(
            !same_file(path, output)?,
            "PNG and PBM output paths must differ"
        );
    }
    for destination in std::iter::once(output).chain(pbm) {
        for source in std::iter::once(layout).chain(data) {
            ensure!(
                !same_file(destination, source)?,
                "output must not replace a layout or snapshot input"
            );
        }
    }
    let layout: Layout = read_json(layout)?;
    let data: SnapshotSet = data.map(read_json).transpose()?.unwrap_or(SnapshotSet {
        schema_version: 1,
        snapshots: Vec::new(),
    });
    let now = match now {
        Some(now) => now,
        None => SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    };
    let started = Instant::now();
    let rendered = Renderer::new()?.render(&layout, &data, now)?;
    let render_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut png = BufWriter::new(File::create(output).context("creating preview PNG")?);
    rendered.frame.write_png(&mut png)?;
    png.flush()?;
    if let Some(path) = pbm {
        let mut file = BufWriter::new(File::create(path).context("creating packed PBM")?);
        rendered.frame.write_pbm(&mut file)?;
        file.flush()?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "display_status": "simulated",
            "display_id": layout.display.id,
            "width": rendered.frame.width(),
            "height": rendered.frame.height(),
            "packed_bytes": rendered.frame.bits().len(),
            "render_ms": render_ms,
            "components": rendered.components,
        }))?
    );
    Ok(())
}
