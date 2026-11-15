//! Named plugin-relative component designs and renderer-measured compact sizing.
use std::{collections::BTreeSet, path::Path};

use anyhow::{Result, ensure};
use axum::{
    Json,
    extract::{Path as RoutePath, State},
    http::StatusCode,
};
use paperphase_core::{
    Component, ContentLayout, DisplaySpec, IconReference, Layout, MAX_PIXELS, Rect, STATUS_HEIGHT,
    SnapshotSet, TextSource, valid_id,
};
use paperphase_render::{RenderWarning, Renderer};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ApiError, ApiResult, Shared, internal, invalid, persist};

const SOURCE: &str = "design.source";
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Design {
    pub id: String,
    pub connector_id: String,
    pub name: String,
    pub component: Component,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Store {
    schema_version: u32,
    revision: u64,
    designs: Vec<Design>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            designs: vec![],
        }
    }
}
fn scene(component: Component) -> Layout {
    Layout {
        schema_version: 1,
        display: DisplaySpec {
            id: "component-design".into(),
            width: component.rect.width,
            height: component.rect.height,
        },
        components: vec![component],
    }
}
fn normalize(component: &mut Component) {
    component.id = "design.component".into();
    component.rect.x = 0;
    component.rect.y = 0;
    for block in &mut component.blocks {
        match &mut block.source {
            TextSource::Field { binding }
            | TextSource::Icon {
                icon: IconReference::Field { binding },
                ..
            } => binding.connection_id = SOURCE.into(),
            _ => {}
        }
    }
}
fn validate(design: &Design) -> Result<()> {
    ensure!(
        design.component.rotation.is_none(),
        "Save individual rotating items as reusable designs, not the whole area"
    );
    ensure!(
        valid_id(&design.id) && valid_id(&design.connector_id),
        "invalid design or plugin identifier"
    );
    ensure!(
        !design.name.trim().is_empty() && design.name.len() <= 128,
        "design name must be 1–128 bytes"
    );
    ensure!(
        design.component.rect.x == 0 && design.component.rect.y == 0,
        "design coordinates must be relative"
    );
    scene(design.component.clone()).validate()?;
    ensure!(
        design
            .component
            .blocks
            .iter()
            .filter_map(|block| block.source.binding())
            .all(|binding| binding.connection_id == SOURCE),
        "design bindings must be plugin-relative"
    );
    Ok(())
}
pub(super) fn load(path: &Path) -> Result<Store> {
    let store: Store = if path.try_exists()? {
        crate::simulator::read_json(path)?
    } else {
        Store::default()
    };
    ensure!(
        store.schema_version == 1 && store.designs.len() <= 128,
        "invalid design store schema or count"
    );
    let mut ids = BTreeSet::new();
    for design in &store.designs {
        ensure!(ids.insert(&design.id), "duplicate design identifier");
        validate(design)?;
    }
    Ok(store)
}
pub(super) async fn list(State(app): State<Shared>) -> Json<Store> {
    Json(app.designs.lock().await.clone())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Write {
    base_revision: u64,
    connector_id: String,
    name: String,
    component: Component,
}
pub(super) async fn save(
    State(app): State<Shared>,
    RoutePath(id): RoutePath<String>,
    Json(request): Json<Write>,
) -> ApiResult<Json<Store>> {
    let mut design = Design {
        id,
        connector_id: request.connector_id,
        name: request.name.trim().to_owned(),
        component: request.component,
    };
    normalize(&mut design.component);
    validate(&design).map_err(invalid)?;
    if design.connector_id != "paperphase.builtin"
        && app.connector_catalog.get(&design.connector_id).is_none()
    {
        return Err(invalid(
            "This plugin is unavailable. Your design has not been saved.",
        ));
    }
    let mut store = app.designs.lock().await;
    if request.base_revision != store.revision {
        return Err(ApiError(StatusCode::CONFLICT, "The design library changed in another tab. Reload the library and try saving again; your draft is retained.".into()));
    }
    let mut updated = store.clone();
    if let Some(existing) = updated
        .designs
        .iter_mut()
        .find(|existing| existing.id == design.id)
    {
        *existing = design;
    } else {
        if updated.designs.len() >= 128 {
            return Err(invalid(
                "The design library is full. Remove an unused design first.",
            ));
        }
        updated.designs.push(design);
    }
    updated.revision = updated
        .revision
        .checked_add(1)
        .ok_or_else(|| internal("design revision exhausted"))?;
    let file = app.designs_file.clone();
    let bytes = updated.clone();
    tokio::task::spawn_blocking(move || persist(&file, &bytes))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    *store = updated;
    Ok(Json(store.clone()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Remove {
    base_revision: u64,
}
pub(super) async fn remove(
    State(app): State<Shared>,
    RoutePath(id): RoutePath<String>,
    Json(request): Json<Remove>,
) -> ApiResult<Json<Store>> {
    let mut store = app.designs.lock().await;
    if request.base_revision != store.revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "The design library changed. Reload it and try again.".into(),
        ));
    }
    if !store.designs.iter().any(|design| design.id == id) {
        return Err(ApiError(StatusCode::NOT_FOUND, "Design not found.".into()));
    }
    let mut updated = store.clone();
    updated.designs.retain(|design| design.id != id);
    updated.revision = updated
        .revision
        .checked_add(1)
        .ok_or_else(|| internal("design revision exhausted"))?;
    let file = app.designs_file.clone();
    let bytes = updated.clone();
    tokio::task::spawn_blocking(move || persist(&file, &bytes))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    *store = updated;
    Ok(Json(store.clone()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Fit {
    component: Component,
    width: Option<u32>,
}
// Fitting uses the renderer's font/data semantics; never browser font estimates.
fn fit_component(
    renderer: &mut Renderer,
    component: &Component,
    data: &SnapshotSet,
    at: u64,
    panel: &DisplaySpec,
    fixed_width: Option<u32>,
) -> Result<Component> {
    let mut candidate = component.clone();
    candidate.rect.x = 0;
    candidate.rect.y = 0;
    if candidate.content_layout == ContentLayout::Positioned {
        let trim = |component: &mut Component| {
            let (width, height) = component.blocks.iter().filter_map(|block| block.rect).fold(
                (0u32, 0u32),
                |(w, h), rect| {
                    (
                        w.max(rect.x.saturating_add(rect.width)),
                        h.max(rect.y.saturating_add(rect.height)),
                    )
                },
            );
            let padding = 2 * u32::from(component.padding);
            component.rect.width = width.saturating_add(padding).max(32);
            component.rect.height = height
                .saturating_add(padding)
                .saturating_add(if component.has_bindings() {
                    STATUS_HEIGHT
                } else {
                    0
                })
                .max(32);
        };
        trim(&mut candidate);
        scene(candidate.clone()).validate()?;
        let rendered = renderer.render(&scene(candidate.clone()), data, at)?;
        for (block, report) in candidate
            .blocks
            .iter_mut()
            .zip(&rendered.components[0].blocks)
        {
            if !report.overflow {
                continue;
            }
            if let (Some(rect), Some(size)) = (block.rect.as_mut(), report.content_size.as_ref()) {
                rect.width = rect.width.max(
                    if matches!(block.overflow, paperphase_core::Overflow::Wrap) {
                        size.minimum_width
                    } else {
                        size.width
                    },
                );
                rect.height = rect.height.max(size.height);
            }
        }
        // Items share a redraw area: growing a clipped box may overlap its neighbour.
        // Keep the owner's positions and typography, then fit the outer component.
        trim(&mut candidate);
        scene(candidate.clone()).validate()?;
        return Ok(candidate);
    }
    let width_limit = panel.width.max(32);
    if let Some(width) = fixed_width {
        ensure!((32..=4096).contains(&width), "width must be 32–4096 pixels");
    }
    let mut widths = vec![fixed_width.unwrap_or(width_limit)];
    if fixed_width.is_none() {
        widths.extend(
            [96, 128, 160, 192, 240, 320, 480, 640]
                .into_iter()
                .filter(|width| *width <= width_limit),
        );
    }
    let mut best: Option<(u64, Component)> = None;
    let mut minimum_width = 2 * u32::from(candidate.padding) + 1;
    let mut index = 0;
    while index < widths.len() {
        let width = widths[index];
        index += 1;
        if width < minimum_width {
            continue;
        }
        candidate.rect = Rect {
            x: 0,
            y: 0,
            width,
            height: 4096.min((MAX_PIXELS / u64::from(width)) as u32),
        };
        let rendered = renderer.render(&scene(candidate.clone()), data, at)?;
        let report = &rendered.components[0];
        minimum_width = report
            .blocks
            .iter()
            .filter_map(|block| block.content_size.as_ref())
            .map(|size| size.minimum_width)
            .max()
            .unwrap_or(0)
            .saturating_add(2 * u32::from(candidate.padding));
        if index == 1 && fixed_width.is_none() {
            let natural = report
                .blocks
                .iter()
                .filter_map(|block| block.content_size.as_ref())
                .map(|size| size.width)
                .max()
                .unwrap_or(24)
                .saturating_add(2 * u32::from(candidate.padding))
                .clamp(32, width_limit);
            if !widths.contains(&natural) {
                widths.push(natural);
            }
        }
        if width < minimum_width {
            continue;
        }
        if report.warnings.contains(&RenderWarning::Overflow)
            || report.blocks.iter().any(|block| block.rect.is_none())
        {
            continue;
        }
        let height = report
            .blocks
            .iter()
            .filter_map(|block| block.rect.zip(block.content_size.as_ref()))
            .map(|(rect, size)| rect.y.saturating_add(size.height))
            .max()
            .unwrap_or(0)
            .saturating_add(2 * u32::from(candidate.padding))
            .saturating_add(if candidate.has_bindings() {
                STATUS_HEIGHT
            } else {
                0
            })
            .max(32);
        if height > candidate.rect.height {
            continue;
        }
        let mut fitted = candidate.clone();
        fitted.rect.height = height;
        // Keep compact area, but avoid tall strips caused by repeatedly wrapping
        // long values/attribution. Explicit fixed widths remain the owner's choice.
        let area = u64::from(width) * u64::from(height);
        let inner_width = width
            .saturating_sub(2 * u32::from(candidate.padding))
            .max(1);
        let wrap_excess = report
            .blocks
            .iter()
            .filter_map(|block| block.content_size.as_ref())
            .map(|size| size.width.saturating_sub(inner_width.saturating_mul(2)))
            .max()
            .unwrap_or(0);
        let wrap_penalty = if fixed_width.is_none() {
            area * u64::from(wrap_excess) / (2 * u64::from(inner_width))
        } else {
            0
        };
        let score = area
            + wrap_penalty
            + if width > panel.width || height > panel.height {
                MAX_PIXELS * 16
            } else {
                0
            };
        if best.as_ref().is_none_or(|(old, _)| score < *old) {
            best = Some((score, fitted));
        }
    }
    let fitted = best.map(|(_,component)|component).ok_or_else(|| anyhow::anyhow!("There is too much content to fit. Remove an item, use smaller text, or increase the design width."))?;
    scene(fitted.clone()).validate()?;
    Ok(fitted)
}
pub(super) async fn fit(
    State(app): State<Shared>,
    Json(request): Json<Fit>,
) -> ApiResult<Json<Value>> {
    // Validate structure before any potentially expensive measurement. Flow's
    // supplied geometry is replaced by a bounded measurement surface.
    if request.component.rotation.is_some() {
        return Err(invalid(
            "Fit individual rotating items, not the whole area.",
        ));
    }
    let mut check = request.component.clone();
    check.rect = Rect {
        x: 0,
        y: 0,
        width: 4096,
        height: 1024,
    };
    if check.content_layout == ContentLayout::Positioned {
        // A temporarily undersized outer box remains repairable with Fit.
        let (width, height) = check.blocks.iter().filter_map(|block| block.rect).fold(
            (0u32, 0u32),
            |(w, h), rect| {
                (
                    w.max(rect.x.saturating_add(rect.width)),
                    h.max(rect.y.saturating_add(rect.height)),
                )
            },
        );
        let padding = 2 * u32::from(check.padding);
        check.rect.width = width.saturating_add(padding).max(32);
        check.rect.height = height
            .saturating_add(padding)
            .saturating_add(if check.has_bindings() {
                STATUS_HEIGHT
            } else {
                0
            })
            .max(32);
    }
    scene(check).validate().map_err(invalid)?;
    let panel = app.workspace.lock().await.saved.layout.display.clone();
    let data = {
        let store = app.connections.lock().await;
        super::connections::merge_snapshots(&store, &app.snapshots)
    };
    let permit = Arc::clone(&app.render_slots)
        .try_acquire_owned()
        .map_err(|_| {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Renderer busy. Try again shortly.".into(),
            )
        })?;
    let at = (app.clock)();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut renderer = app.renderer.lock().map_err(internal)?;
        let component = fit_component(
            &mut renderer,
            &request.component,
            &data,
            at,
            &panel,
            request.width,
        )
        .map_err(invalid)?;
        Ok(Json(json!({"component":component})))
    })
    .await
    .map_err(internal)?
}
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;
    fn text_component(text: &str, size: u16) -> Component {
        serde_json::from_value(json!({"id":"sample","rect":{"x":0,"y":0,"width":1200,"height":1000},"padding":4,"gap":2,"blocks":[{"id":"text","font_size":size,"source":{"kind":"literal","text":text}}]})).unwrap()
    }
    fn panel() -> DisplaySpec {
        DisplaySpec {
            id: "test".into(),
            width: 800,
            height: 480,
        }
    }
    fn empty() -> SnapshotSet {
        SnapshotSet {
            schema_version: 1,
            snapshots: vec![],
        }
    }
    #[test]
    fn fits_oversized_flow_with_renderer_metrics_without_shrinking_text() {
        let mut renderer = Renderer::new().unwrap();
        let result = fit_component(
            &mut renderer,
            &text_component("18.5 °C", 14),
            &empty(),
            0,
            &panel(),
            None,
        )
        .unwrap();
        assert!(result.rect.width < 120 && result.rect.height <= 40);
        assert_eq!(result.blocks[0].font_size, 14);
        let rendered = renderer.render(&scene(result), &empty(), 0).unwrap();
        assert!(
            !rendered.components[0]
                .warnings
                .contains(&RenderWarning::Overflow)
        );
    }
    #[test]
    fn fixed_width_wraps_all_content_and_big_glyphs_are_not_silently_clipped() {
        let mut renderer = Renderer::new().unwrap();
        let result = fit_component(
            &mut renderer,
            &text_component("A forecast with several useful words on separate lines", 14),
            &empty(),
            0,
            &panel(),
            Some(128),
        )
        .unwrap();
        assert_eq!(result.rect.width, 128);
        assert!(result.rect.height > 32);
        assert!(
            !renderer
                .render(&scene(result), &empty(), 0)
                .unwrap()
                .components[0]
                .warnings
                .contains(&RenderWarning::Overflow)
        );
        let large = fit_component(
            &mut renderer,
            &text_component("W", 256),
            &empty(),
            0,
            &panel(),
            None,
        )
        .unwrap();
        assert!(large.rect.width >= 250);
        assert_eq!(large.blocks[0].font_size, 256);
        assert!(
            fit_component(
                &mut renderer,
                &text_component("W", 256),
                &empty(),
                0,
                &panel(),
                Some(32)
            )
            .is_err()
        );
    }
    #[test]
    fn automatic_fit_avoids_extreme_wrapping_but_honours_an_explicit_width() {
        let mut renderer = Renderer::new().unwrap();
        let component = text_component(
            "A useful forecast summary with attribution preserved for every display component",
            14,
        );
        let auto = fit_component(&mut renderer, &component, &empty(), 0, &panel(), None).unwrap();
        let narrow =
            fit_component(&mut renderer, &component, &empty(), 0, &panel(), Some(96)).unwrap();
        assert!(auto.rect.width > 96);
        assert!(auto.rect.height < narrow.rect.height);
        assert_eq!(narrow.rect.width, 96);
        assert_eq!(auto.blocks[0].font_size, 14);
        assert_eq!(
            serde_json::to_value(&auto.blocks[0].source).unwrap(),
            serde_json::to_value(&component.blocks[0].source).unwrap()
        );
    }
    #[test]
    fn positioned_fit_repairs_clipped_items_without_moving_them() {
        let mut renderer = Renderer::new().unwrap();
        let mut component = text_component("Partly cloudy", 16);
        component.content_layout = ContentLayout::Positioned;
        component.blocks[0].rect = Some(Rect {
            x: 0,
            y: 0,
            width: 96,
            height: 8,
        });
        let mut second = component.blocks[0].clone();
        second.id = "second".into();
        second.rect = Some(Rect {
            x: 0,
            y: 8,
            width: 96,
            height: 8,
        });
        component.blocks.push(second);
        let fitted = fit_component(&mut renderer, &component, &empty(), 0, &panel(), None).unwrap();
        for (before, after) in component.blocks.iter().zip(&fitted.blocks) {
            assert_eq!(before.rect.unwrap().x, after.rect.unwrap().x);
            assert_eq!(before.rect.unwrap().y, after.rect.unwrap().y);
            assert_eq!(before.font_size, after.font_size);
            assert!(after.rect.unwrap().height > 8);
        }
        assert!(
            fitted.blocks[0]
                .rect
                .unwrap()
                .intersects(fitted.blocks[1].rect.unwrap())
        );
        assert!(
            !renderer
                .render(&scene(fitted), &empty(), 0)
                .unwrap()
                .components[0]
                .warnings
                .contains(&RenderWarning::Overflow)
        );
    }
    #[test]
    fn data_band_and_positioned_boxes_remain_reserved() {
        let mut renderer = Renderer::new().unwrap();
        let mut component = text_component("", 14);
        component.blocks[0].source=serde_json::from_value(json!({"kind":"field","binding":{"connection_id":"home","dataset_id":"current","field_id":"temperature"}})).unwrap();
        let result = fit_component(&mut renderer, &component, &empty(), 0, &panel(), None).unwrap();
        assert!(result.rect.height > 32);
        assert!(result.rect.height > STATUS_HEIGHT + 8);
        let mut positioned = text_component("Value", 14);
        positioned.content_layout = ContentLayout::Positioned;
        positioned.blocks[0].rect = Some(Rect {
            x: 12,
            y: 20,
            width: 80,
            height: 24,
        });
        let result =
            fit_component(&mut renderer, &positioned, &empty(), 0, &panel(), None).unwrap();
        assert_eq!(result.rect.width, 100);
        assert_eq!(result.rect.height, 52);
        assert_eq!(result.blocks[0].rect, positioned.blocks[0].rect);
    }
    #[test]
    fn stored_designs_normalize_binding_ids_and_reject_invalid_geometry() {
        let mut component = text_component("", 14);
        component.blocks[0].source=serde_json::from_value(json!({"kind":"icon","pack_id":"weather","size":24,"icon":{"kind":"field","binding":{"connection_id":"private.source","dataset_id":"current","field_id":"icon"}}})).unwrap();
        normalize(&mut component);
        assert_eq!(
            component.blocks[0].source.binding().unwrap().connection_id,
            SOURCE
        );
        let mut design = Design {
            id: "my-design".into(),
            connector_id: "weather".into(),
            name: "Compact".into(),
            component,
        };
        validate(&design).unwrap();
        design.component.rect.width = 8;
        assert!(validate(&design).is_err());
    }
}
