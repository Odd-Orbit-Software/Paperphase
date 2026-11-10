use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Dataset, Result, require, valid_id};

pub const MAX_PIXELS: u64 = 4_194_304;
pub const STATUS_HEIGHT: u32 = 18;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DisplaySpec {
    /// A simulator profile, not an automatically detected hardware model.
    pub id: String,
    pub width: u32,
    pub height: u32,
}

impl DisplaySpec {
    pub fn validate(&self) -> Result<()> {
        require(valid_id(&self.id), "display.id", "invalid identifier")?;
        require(
            self.width > 0 && self.height > 0 && self.width <= 4096 && self.height <= 4096,
            "display",
            "dimensions must be 1–4096 pixels",
        )?;
        require(
            u64::from(self.width) * u64::from(self.height) <= MAX_PIXELS,
            "display",
            "pixel budget exceeded",
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn right(self) -> u64 {
        u64::from(self.x) + u64::from(self.width)
    }
    pub fn bottom(self) -> u64 {
        u64::from(self.y) + u64::from(self.height)
    }
    pub fn intersects(self, other: Self) -> bool {
        self.width > 0
            && self.height > 0
            && other.width > 0
            && other.height > 0
            && u64::from(self.x) < other.right()
            && u64::from(other.x) < self.right()
            && u64::from(self.y) < other.bottom()
            && u64::from(other.y) < self.bottom()
    }
    pub fn contains(self, x: i64, y: i64) -> bool {
        x >= i64::from(self.x)
            && y >= i64::from(self.y)
            && x < self.right() as i64
            && y < self.bottom() as i64
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub schema_version: u32,
    pub display: DisplaySpec,
    pub components: Vec<Component>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub rect: Rect,
    #[serde(default = "default_padding")]
    pub padding: u16,
    #[serde(default = "default_gap")]
    pub gap: u16,
    #[serde(default)]
    pub border: bool,
    pub blocks: Vec<TextBlock>,
}

fn default_padding() -> u16 {
    12
}
fn default_gap() -> u16 {
    8
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextBlock {
    pub id: String,
    pub source: TextSource,
    pub font_size: u16,
    #[serde(default)]
    pub align: Align,
    #[serde(default)]
    pub overflow: Overflow,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Overflow {
    #[default]
    Wrap,
    Clip,
    Ellipsis,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextSource {
    Literal { text: String },
    Field { binding: Binding },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub connection_id: String,
    pub dataset_id: String,
    pub field_id: String,
    #[serde(default)]
    pub row: usize,
    #[serde(default)]
    pub decimals: Option<u8>,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
}

impl Component {
    pub fn has_bindings(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block.source, TextSource::Field { .. }))
    }
}

impl Layout {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1,
            "layout.schema_version",
            "unsupported schema version",
        )?;
        self.display.validate()?;
        require(
            self.components.len() <= 64,
            "layout.components",
            "component limit exceeded",
        )?;
        let mut ids = BTreeSet::new();
        let mut text_bytes = 0;
        for (i, component) in self.components.iter().enumerate() {
            let path = format!("layout.components[{i}]");
            require(
                valid_id(&component.id),
                format!("{path}.id"),
                "invalid identifier",
            )?;
            require(
                ids.insert(&component.id),
                format!("{path}.id"),
                "duplicate identifier",
            )?;
            let rect = component.rect;
            require(
                rect.width >= 32 && rect.height >= 32,
                format!("{path}.rect"),
                "minimum component size is 32 × 32",
            )?;
            require(
                rect.right() <= u64::from(self.display.width)
                    && rect.bottom() <= u64::from(self.display.height),
                format!("{path}.rect"),
                "component extends outside the display",
            )?;
            require(
                component.padding <= 64 && component.gap <= 64,
                &path,
                "padding and gap must be at most 64 pixels",
            )?;
            let padding = 2 * u32::from(component.padding);
            let status = if component.has_bindings() {
                STATUS_HEIGHT
            } else {
                0
            };
            require(
                rect.width > padding && rect.height > padding + status,
                format!("{path}.rect"),
                "insufficient space for padding and data status",
            )?;
            require(
                !component.blocks.is_empty() && component.blocks.len() <= 32,
                format!("{path}.blocks"),
                "must have 1–32 text blocks",
            )?;
            for other in &self.components[..i] {
                require(
                    !rect.intersects(other.rect),
                    format!("{path}.rect"),
                    "component overlaps another component",
                )?;
            }
            let mut block_ids = BTreeSet::new();
            for (j, block) in component.blocks.iter().enumerate() {
                let path = format!("{path}.blocks[{j}]");
                require(
                    valid_id(&block.id) && block_ids.insert(&block.id),
                    format!("{path}.id"),
                    "invalid or duplicate identifier",
                )?;
                require(
                    (8..=256).contains(&block.font_size),
                    format!("{path}.font_size"),
                    "font size must be 8–256 pixels",
                )?;
                match &block.source {
                    TextSource::Literal { text } => {
                        require(
                            text.len() <= 4096,
                            format!("{path}.source.text"),
                            "text byte limit exceeded",
                        )?;
                        text_bytes += text.len();
                    }
                    TextSource::Field { binding } => {
                        for (name, id) in [
                            ("connection_id", &binding.connection_id),
                            ("dataset_id", &binding.dataset_id),
                            ("field_id", &binding.field_id),
                        ] {
                            require(
                                valid_id(id),
                                format!("{path}.source.binding.{name}"),
                                "invalid identifier",
                            )?;
                        }
                        require(
                            binding.row < 512,
                            format!("{path}.source.binding.row"),
                            "row index must be below 512",
                        )?;
                        require(
                            binding.decimals.is_none_or(|n| n <= 6),
                            &path,
                            "decimal precision must be at most six",
                        )?;
                        require(
                            binding.prefix.len() <= 128 && binding.suffix.len() <= 128,
                            &path,
                            "format prefix/suffix byte limit exceeded",
                        )?;
                        text_bytes += binding.prefix.len() + binding.suffix.len();
                    }
                }
            }
        }
        require(
            text_bytes <= 65536,
            "layout",
            "total text byte limit exceeded",
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub connection_id: String,
    pub dataset: Dataset,
    pub last_success_at: u64,
    #[serde(default)]
    pub observed_at: Option<u64>,
    pub stale_after_secs: u64,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Snapshot {
    pub fn is_stale(&self, now: u64) -> bool {
        self.last_error.is_some()
            || now < self.last_success_at
            || self.observed_at.is_some_and(|observed| observed > now)
            || now.saturating_sub(self.observed_at.map_or(self.last_success_at, |observed| {
                observed.min(self.last_success_at)
            })) >= self.stale_after_secs
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotSet {
    pub schema_version: u32,
    pub snapshots: Vec<Snapshot>,
}

impl SnapshotSet {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1,
            "snapshots.schema_version",
            "unsupported schema version",
        )?;
        require(
            self.snapshots.len() <= 256,
            "snapshots",
            "snapshot limit exceeded",
        )?;
        let mut keys = BTreeSet::new();
        for (i, snapshot) in self.snapshots.iter().enumerate() {
            let path = format!("snapshots[{i}]");
            require(
                valid_id(&snapshot.connection_id),
                format!("{path}.connection_id"),
                "invalid identifier",
            )?;
            snapshot.dataset.validate().map_err(|mut error| {
                error.path = format!("{path}.{}", error.path);
                error
            })?;
            require(
                keys.insert((&snapshot.connection_id, &snapshot.dataset.id)),
                &path,
                "duplicate connection/dataset key",
            )?;
            require(
                snapshot.stale_after_secs > 0,
                &path,
                "stale interval must be positive",
            )?;
            require(
                snapshot.last_error.as_ref().is_none_or(|s| s.len() <= 1024),
                &path,
                "error text byte limit exceeded",
            )?;
        }
        Ok(())
    }
}
