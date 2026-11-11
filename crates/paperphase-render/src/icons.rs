//! Plugin-owned PNG sprite packs. No file paths or image URLs enter layouts.
use anyhow::{Context, Result, ensure};
use paperphase_core::valid_id;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read},
    path::{Component, Path},
};

const MAX_MANIFEST: usize = 64 * 1024;
const MAX_PNG: usize = 1024 * 1024;
const MAX_PIXELS: u64 = 4_194_304;
const MAX_STORED: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    id: String,
    name: String,
    variants: Vec<Variant>,
    icons: BTreeMap<String, Cell>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    size: u16,
    image: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cell {
    column: u16,
    row: u16,
}
#[derive(Clone, Serialize)]
pub struct IconPackInfo {
    pub id: String,
    pub name: String,
    pub icons: Vec<String>,
    pub sizes: Vec<u16>,
}
#[derive(Clone)]
pub(crate) struct Sprite {
    pub size: u16,
    bits: Vec<u8>,
}
impl Sprite {
    pub fn black(&self, x: u32, y: u32) -> bool {
        let stride = u32::from(self.size).div_ceil(8) as usize;
        self.bits[y as usize * stride + x as usize / 8] & (0x80 >> (x % 8)) != 0
    }
}
#[derive(Clone, Default)]
pub struct IconStore {
    packs: BTreeMap<String, IconPackInfo>,
    sprites: BTreeMap<(String, String), Vec<Sprite>>,
    bytes: usize,
}
impl IconStore {
    pub fn load_pack(path: &Path, expected_id: &str) -> Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take((MAX_MANIFEST + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_MANIFEST, "icon manifest exceeds budget");
        let manifest: Manifest = serde_json::from_slice(&bytes).context("invalid icon manifest")?;
        ensure!(
            manifest.schema_version == 1 && manifest.id == expected_id && valid_id(&manifest.id),
            "invalid icon pack identity"
        );
        ensure!(
            !manifest.name.trim().is_empty() && manifest.name.len() <= 96,
            "invalid icon pack name"
        );
        ensure!(
            !manifest.icons.is_empty()
                && manifest.icons.len() <= 128
                && manifest.icons.keys().all(|key| valid_id(key)),
            "invalid icon identifiers or count"
        );
        ensure!(
            !manifest.variants.is_empty() && manifest.variants.len() <= 8,
            "invalid icon variant count"
        );
        let root = path
            .parent()
            .context("manifest needs a parent directory")?
            .canonicalize()?;
        let mut store = Self::default();
        let mut sizes = BTreeSet::new();
        for variant in &manifest.variants {
            ensure!(
                (8..=256).contains(&variant.size) && sizes.insert(variant.size),
                "invalid or duplicate icon size"
            );
            let relative = Path::new(&variant.image);
            ensure!(
                !relative.as_os_str().is_empty()
                    && relative
                        .components()
                        .all(|part| matches!(part, Component::Normal(_))),
                "icon image must be a relative path within its pack"
            );
            let image_path = root.join(relative).canonicalize()?;
            ensure!(image_path.starts_with(&root), "icon image leaves its pack");
            let mut bytes = Vec::new();
            File::open(image_path)?
                .take((MAX_PNG + 1) as u64)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= MAX_PNG, "icon image exceeds file budget");
            let mut decoder = png::Decoder::new(BufReader::new(std::io::Cursor::new(bytes)));
            decoder.set_limits(png::Limits {
                bytes: 32 * 1024 * 1024,
            });
            decoder
                .set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let mut reader = decoder.read_info()?;
            let info = reader.info();
            ensure!(
                info.width > 0
                    && info.height > 0
                    && u64::from(info.width) * u64::from(info.height) <= MAX_PIXELS,
                "icon sheet exceeds pixel budget"
            );
            ensure!(
                info.animation_control.is_none(),
                "animated icon sheets are unsupported"
            );
            let mut output = vec![
                0;
                reader
                    .output_buffer_size()
                    .context("invalid image output size")?
            ];
            let frame = reader.next_frame(&mut output)?;
            ensure!(
                frame.bit_depth == png::BitDepth::Eight,
                "unsupported icon bit depth"
            );
            let channels = match frame.color_type {
                png::ColorType::Grayscale => 1,
                png::ColorType::GrayscaleAlpha => 2,
                png::ColorType::Rgb => 3,
                png::ColorType::Rgba => 4,
                _ => anyhow::bail!("unsupported icon colour type"),
            };
            let size = u32::from(variant.size);
            let stride = size.div_ceil(8) as usize;
            for (key, cell) in &manifest.icons {
                let left = u32::from(cell.column) * size;
                let top = u32::from(cell.row) * size;
                ensure!(
                    left + size <= frame.width && top + size <= frame.height,
                    "icon cell leaves its sheet"
                );
                let mut bits = vec![0; stride * size as usize];
                for y in 0..size {
                    for x in 0..size {
                        let offset = ((top + y) as usize * frame.width as usize
                            + (left + x) as usize)
                            * channels;
                        let pixel = &output[offset..offset + channels];
                        let (red, green, blue, alpha) = match channels {
                            1 => (pixel[0], pixel[0], pixel[0], 255),
                            2 => (pixel[0], pixel[0], pixel[0], pixel[1]),
                            3 => (pixel[0], pixel[1], pixel[2], 255),
                            _ => (pixel[0], pixel[1], pixel[2], pixel[3]),
                        };
                        let luminance =
                            (u32::from(red) * 299 + u32::from(green) * 587 + u32::from(blue) * 114)
                                / 1000;
                        if alpha >= 128 && luminance < 128 {
                            bits[y as usize * stride + x as usize / 8] |= 0x80 >> (x % 8);
                        }
                    }
                }
                store.bytes += bits.len();
                ensure!(store.bytes <= MAX_STORED, "icon pixel budget exceeded");
                store
                    .sprites
                    .entry((manifest.id.clone(), key.clone()))
                    .or_default()
                    .push(Sprite {
                        size: variant.size,
                        bits,
                    });
            }
        }
        store.packs.insert(
            manifest.id.clone(),
            IconPackInfo {
                id: manifest.id,
                name: manifest.name,
                icons: manifest.icons.into_keys().collect(),
                sizes: sizes.into_iter().collect(),
            },
        );
        Ok(store)
    }
    pub fn merge(&mut self, other: Self) -> Result<()> {
        ensure!(
            self.bytes + other.bytes <= MAX_STORED,
            "combined icon pixel budget exceeded"
        );
        ensure!(
            other.packs.keys().all(|id| !self.packs.contains_key(id)),
            "duplicate icon pack"
        );
        self.bytes += other.bytes;
        self.packs.extend(other.packs);
        self.sprites.extend(other.sprites);
        Ok(())
    }
    pub fn catalog(&self) -> Vec<IconPackInfo> {
        self.packs.values().cloned().collect()
    }
    pub(crate) fn get(&self, pack: &str, icon: &str, size: u16) -> Option<&Sprite> {
        self.sprites
            .get(&(pack.to_owned(), icon.to_owned()))?
            .iter()
            .min_by_key(|sprite| (sprite.size.abs_diff(size), std::cmp::Reverse(sprite.size)))
    }
}
