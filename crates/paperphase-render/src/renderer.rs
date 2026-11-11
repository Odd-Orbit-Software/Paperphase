use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, anyhow};
use fontdue::{
    Font, FontSettings, Metrics,
    layout::{
        CoordinateSystem, GlyphRasterConfig, HorizontalAlign, Layout as TextLayout, LayoutSettings,
        TextStyle,
    },
};
use paperphase_core::{
    Align, Binding, Component, ContentLayout, FieldKind, Layout, Overflow, Rect, STATUS_HEIGHT,
    Snapshot, SnapshotSet, TextBlock, TextSource,
};
use serde::Serialize;

use crate::Frame;

const FONT: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSans.ttf");
const CACHE_BYTES: usize = 2 * 1024 * 1024;
const CACHE_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataState {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderWarning {
    MissingDataset,
    MissingField,
    MissingRow,
    NullValue,
    IncompatibleFormat,
    GlyphFallback,
    Overflow,
}

#[derive(Debug, Serialize)]
pub struct ComponentReport {
    pub id: String,
    pub state: DataState,
    pub warnings: Vec<RenderWarning>,
    pub blocks: Vec<BlockReport>,
}

#[derive(Debug, Serialize)]
pub struct BlockReport {
    pub id: String,
    /// The visible box in content-local coordinates; None for hidden flow blocks.
    pub rect: Option<Rect>,
}

struct TextMetrics {
    flow_height: u32,
    visible_height: u32,
    overflow: bool,
    fallback: bool,
}

impl ComponentReport {
    fn warn(&mut self, warning: RenderWarning) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }
    fn missing(&mut self, warning: RenderWarning) -> String {
        self.state = DataState::Unavailable;
        self.warn(warning);
        "—".into()
    }
}

pub struct RenderedFrame {
    pub frame: Frame,
    pub components: Vec<ComponentReport>,
}

pub struct Renderer {
    font: Font,
    flow: TextLayout,
    cache: HashMap<GlyphRasterConfig, (Metrics, Vec<u8>)>,
    cache_bytes: usize,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let font = Font::from_bytes(FONT, FontSettings::default()).map_err(|e| anyhow!(e))?;
        Ok(Self {
            font,
            flow: TextLayout::new(CoordinateSystem::PositiveYDown),
            cache: HashMap::new(),
            cache_bytes: 0,
        })
    }

    pub fn render(
        &mut self,
        layout: &Layout,
        data: &SnapshotSet,
        now: u64,
    ) -> Result<RenderedFrame> {
        layout.validate()?;
        data.validate()?;
        let mut frame = Frame::blank(&layout.display)?;
        let index: BTreeMap<_, _> = data
            .snapshots
            .iter()
            .map(|snapshot| {
                (
                    (
                        snapshot.connection_id.as_str(),
                        snapshot.dataset.id.as_str(),
                    ),
                    snapshot,
                )
            })
            .collect();
        let mut reports = Vec::new();
        for component in &layout.components {
            let mut report = ComponentReport {
                id: component.id.clone(),
                state: DataState::Fresh,
                warnings: Vec::new(),
                blocks: component
                    .blocks
                    .iter()
                    .map(|block| BlockReport {
                        id: block.id.clone(),
                        rect: None,
                    })
                    .collect(),
            };
            let values: Vec<_> = component
                .blocks
                .iter()
                .map(|block| match &block.source {
                    TextSource::Literal { text } => Ok(text.clone()),
                    TextSource::Field { binding } => Ok(resolve(binding, &index, now, &mut report)),
                    TextSource::Clock { clock } => clock.format(now).map_err(Into::into),
                })
                .collect::<Result<_>>()?;
            self.draw_component(&mut frame, component, &values, &mut report);
            reports.push(report);
        }
        Ok(RenderedFrame {
            frame,
            components: reports,
        })
    }

    fn draw_component(
        &mut self,
        frame: &mut Frame,
        component: &Component,
        values: &[String],
        report: &mut ComponentReport,
    ) {
        if component.border {
            frame.outline(component.rect);
        }
        let padding = u32::from(component.padding);
        let inner = Rect {
            x: component.rect.x + padding,
            y: component.rect.y + padding,
            width: component.rect.width - 2 * padding,
            height: component.rect.height - 2 * padding,
        };
        let reserve = if component.has_bindings() {
            STATUS_HEIGHT
        } else {
            0
        };
        let body = Rect {
            height: inner.height - reserve,
            ..inner
        };
        let mut y = body.y;
        for (index, (block, value)) in component.blocks.iter().zip(values).enumerate() {
            if component.content_layout == ContentLayout::Flow && u64::from(y) >= body.bottom() {
                report.warn(RenderWarning::Overflow);
                break;
            }
            let clip = if component.content_layout == ContentLayout::Positioned {
                let rect = block
                    .rect
                    .expect("positioned rectangles validated before rendering");
                Rect {
                    x: body.x + rect.x,
                    y: body.y + rect.y,
                    width: rect.width,
                    height: rect.height,
                }
            } else {
                Rect {
                    y,
                    height: (body.bottom() - u64::from(y)) as u32,
                    ..body
                }
            };
            let metrics = self.text(frame, value, block, clip);
            report.blocks[index].rect =
                Some(if component.content_layout == ContentLayout::Positioned {
                    block.rect.expect("validated")
                } else {
                    Rect {
                        x: 0,
                        y: y - body.y,
                        width: body.width,
                        height: metrics.visible_height.min(clip.height),
                    }
                });
            if metrics.overflow {
                report.warn(RenderWarning::Overflow);
            }
            if metrics.fallback {
                report.warn(RenderWarning::GlyphFallback);
            }
            y = y
                .saturating_add(metrics.flow_height)
                .saturating_add(u32::from(component.gap));
        }
        let label = match report.state {
            DataState::Fresh => None,
            DataState::Stale => Some("STALE DATA"),
            DataState::Unavailable => Some("NO DATA"),
        };
        if let Some(label) = label {
            let clip = Rect {
                y: body.bottom() as u32 + 2,
                height: STATUS_HEIGHT - 2,
                ..inner
            };
            let style = TextBlock {
                id: "host-data-status".into(),
                source: TextSource::Literal {
                    text: String::new(),
                },
                font_size: 11,
                align: Align::Left,
                overflow: Overflow::Clip,
                rect: None,
            };
            self.text(frame, label, &style, clip);
        }
    }

    /// Preserve flow advance while also reporting a box covering visible glyphs.
    fn text(
        &mut self,
        frame: &mut Frame,
        text: &str,
        style: &TextBlock,
        clip: Rect,
    ) -> TextMetrics {
        let mut fallback = false;
        let mut text: String = text
            .chars()
            .filter_map(|ch| {
                if ch == '\r' {
                    return None;
                }
                let ch = if ch == '\t' { ' ' } else { ch };
                if ch != '\n' && self.font.lookup_glyph_index(ch) == 0 {
                    fallback = true;
                    Some('\u{fffd}')
                } else {
                    Some(ch)
                }
            })
            .collect();
        let px = f32::from(style.font_size);
        let width = clip.width as f32;
        let wrap = matches!(style.overflow, Overflow::Wrap);
        let mut overflow = false;
        if !wrap {
            text = text.replace('\n', " ");
            if measure(&self.font, &text, px) > width {
                overflow = true;
                if matches!(style.overflow, Overflow::Ellipsis) {
                    let end = measure(&self.font, "…", px);
                    let mut shortened = String::new();
                    let mut advance = 0.0;
                    if end <= width {
                        for ch in text.chars() {
                            let next = self.font.metrics(ch, px).advance_width.ceil();
                            if advance + next + end > width {
                                break;
                            }
                            advance += next;
                            shortened.push(ch);
                        }
                        shortened.push('…');
                    }
                    text = shortened;
                }
            }
        }
        let align = match style.align {
            Align::Left => HorizontalAlign::Left,
            Align::Center => HorizontalAlign::Center,
            Align::Right => HorizontalAlign::Right,
        };
        let offset = if wrap {
            0.0
        } else {
            let space = (width - measure(&self.font, &text, px)).max(0.0);
            match style.align {
                Align::Left => 0.0,
                Align::Center => space / 2.0,
                Align::Right => space,
            }
        };
        self.flow.reset(&LayoutSettings {
            x: clip.x as f32 + offset,
            y: clip.y as f32,
            max_width: wrap.then_some(width),
            horizontal_align: align,
            line_height: 1.15,
            ..LayoutSettings::default()
        });
        self.flow
            .append(&[&self.font], &TextStyle::new(&text, px, 0));
        let height = self.flow.height().ceil() as u32;
        let mut visible_height = height;
        overflow |= height > clip.height;
        for glyph in self.flow.glyphs() {
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }
            let x = glyph.x.round() as i64;
            let y = glyph.y.round() as i64;
            visible_height =
                visible_height.max((y + glyph.height as i64 - i64::from(clip.y)).max(0) as u32);
            if x >= clip.right() as i64
                || y >= clip.bottom() as i64
                || x + glyph.width as i64 <= i64::from(clip.x)
                || y + glyph.height as i64 <= i64::from(clip.y)
            {
                continue;
            }
            if !self.cache.contains_key(&glyph.key) {
                let (metrics, bitmap) = self.font.rasterize_config(glyph.key);
                if self.cache.len() >= CACHE_ENTRIES
                    || self.cache_bytes + bitmap.len() > CACHE_BYTES
                {
                    self.cache.clear();
                    self.cache_bytes = 0;
                }
                self.cache_bytes += bitmap.len();
                self.cache.insert(glyph.key, (metrics, bitmap));
            }
            let (metrics, bitmap) = &self.cache[&glyph.key];
            for row in 0..metrics.height {
                for column in 0..metrics.width {
                    if bitmap[row * metrics.width + column] >= 128 {
                        frame.ink(x + column as i64, y + row as i64, clip);
                    }
                }
            }
        }
        TextMetrics {
            flow_height: height,
            visible_height,
            overflow,
            fallback,
        }
    }
}

fn measure(font: &Font, text: &str, px: f32) -> f32 {
    text.chars()
        .map(|ch| font.metrics(ch, px).advance_width.ceil())
        .sum()
}

fn resolve(
    binding: &Binding,
    index: &BTreeMap<(&str, &str), &Snapshot>,
    now: u64,
    report: &mut ComponentReport,
) -> String {
    let Some(snapshot) = index.get(&(binding.connection_id.as_str(), binding.dataset_id.as_str()))
    else {
        return report.missing(RenderWarning::MissingDataset);
    };
    if snapshot.is_stale(now) {
        report.state = report.state.max(DataState::Stale);
    }
    let Some(field) = snapshot
        .dataset
        .fields
        .iter()
        .find(|field| field.id == binding.field_id)
    else {
        return report.missing(RenderWarning::MissingField);
    };
    if snapshot.dataset.rows.is_empty() {
        return "No items".into();
    }
    let Some(row) = snapshot.dataset.rows.get(binding.row) else {
        return report.missing(RenderWarning::MissingRow);
    };
    let value = &row[&binding.field_id];
    if value.is_null() {
        return report.missing(RenderWarning::NullValue);
    }
    if binding.decimals.is_some() && field.kind != FieldKind::Number {
        return report.missing(RenderWarning::IncompatibleFormat);
    }
    let value = if let Some(decimals) = binding.decimals {
        format!(
            "{:.*}",
            decimals as usize,
            value.as_f64().expect("validated number")
        )
    } else if let Some(text) = value.as_str() {
        text.to_owned()
    } else {
        value.to_string()
    };
    format!("{}{value}{}", binding.prefix, binding.suffix)
}
