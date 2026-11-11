use std::io::Cursor;

use paperphase_core::{Layout, Rect, SnapshotSet};
use paperphase_render::{DataState, Frame, RenderWarning, Renderer};
use serde_json::json;

fn layout() -> Layout {
    serde_json::from_value(json!({"schema_version":1,
        "display":{"id":"test","width":192,"height":96},
        "components":[{"id":"weather","rect":{"x":0,"y":0,"width":96,"height":96},"padding":4,"gap":2,"border":true,
            "blocks":[{"id":"value","font_size":20,"source":{"kind":"field","binding":{
                "connection_id":"home","dataset_id":"current","field_id":"temperature","decimals":1
            }}}]}]
    })).unwrap()
}

fn data() -> SnapshotSet {
    serde_json::from_value(json!({"schema_version":1,"snapshots":[{
        "connection_id":"home","last_success_at":100,"stale_after_secs":60,
        "dataset":{"schema_version":1,"id":"current","fields":[{"id":"temperature","kind":"number"}],"rows":[{"temperature":18.5}]}
    }]})).unwrap()
}

fn literal(text: &str, size: u16, overflow: &str) -> Layout {
    let mut value = serde_json::to_value(layout()).unwrap();
    value["components"][0]["padding"] = json!(0);
    value["components"][0]["border"] = json!(false);
    value["components"][0]["blocks"][0] = json!({"id":"text","font_size":size,"overflow":overflow,"source":{"kind":"literal","text":text}});
    serde_json::from_value(value).unwrap()
}

#[test]
fn clock_and_equivalent_literal_render_identical_pixels_without_a_data_band() {
    let mut value = serde_json::to_value(literal("", 20, "wrap")).unwrap();
    value["components"][0]["blocks"][0]["source"] = json!({"kind":"clock","clock":{"timezone":"Asia/Kolkata","content":"date_time","hour_cycle":"twelve","date_format":"iso"}});
    let scene: Layout = serde_json::from_value(value.clone()).unwrap();
    let at = 1_700_000_000;
    let rendered = Renderer::new()
        .unwrap()
        .render(&scene, &data(), at)
        .unwrap();
    let spec: paperphase_core::ClockSpec =
        serde_json::from_value(value["components"][0]["blocks"][0]["source"]["clock"].clone())
            .unwrap();
    let expected = Renderer::new()
        .unwrap()
        .render(&literal(&spec.format(at).unwrap(), 20, "wrap"), &data(), at)
        .unwrap();
    assert_eq!(rendered.frame.bits(), expected.frame.bits());
    assert_eq!(rendered.components[0].state, DataState::Fresh);
    let later = Renderer::new()
        .unwrap()
        .render(&scene, &data(), at + 60)
        .unwrap();
    assert!(
        rendered
            .frame
            .difference_bounds(&later.frame)
            .unwrap()
            .is_some()
    );
    value["components"][0]["blocks"][0]["source"]["clock"]["timezone"] = json!("Invalid/Zone");
    let invalid: Layout = serde_json::from_value(value).unwrap();
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .path
            .ends_with("source.clock.timezone")
    );
}

#[test]
fn png_and_pbm_represent_exactly_the_same_odd_width_frame() {
    let mut scene = layout();
    scene.display.width = 197;
    scene.components[0].rect.width = 97;
    let rendered = Renderer::new()
        .unwrap()
        .render(&scene, &data(), 101)
        .unwrap();
    let frame = rendered.frame;
    let mut png = Vec::new();
    frame.write_png(&mut png).unwrap();
    let mut decoder = png::Decoder::new(Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(info.color_type, png::ColorType::Grayscale);
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let expected = if frame.is_black(x, y).unwrap() {
                0
            } else {
                255
            };
            assert_eq!(pixels[(y * frame.width() + x) as usize], expected);
        }
    }
    let mut pbm = Vec::new();
    frame.write_pbm(&mut pbm).unwrap();
    let header = format!("P4\n{} {}\n", frame.width(), frame.height());
    assert!(pbm.starts_with(header.as_bytes()));
    assert_eq!(&pbm[header.len()..], frame.bits());
    assert_eq!(frame.bits().len(), 25 * 96);
}

#[test]
fn large_glyphs_are_clipped_to_the_component_rectangle() {
    let scene = literal("MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM", 128, "clip");
    let rendered = Renderer::new()
        .unwrap()
        .render(&scene, &data(), 101)
        .unwrap();
    assert!(
        rendered.components[0]
            .warnings
            .contains(&RenderWarning::Overflow)
    );
    assert!(rendered.frame.bits().iter().any(|byte| *byte != 0));
    for y in 0..96 {
        for x in 96..192 {
            assert_eq!(rendered.frame.is_black(x, y), Some(false));
        }
    }
}

#[test]
fn stale_sources_keep_the_same_content_and_change_only_the_status_band() {
    let scene = layout();
    let mut renderer = Renderer::new().unwrap();
    let fresh = renderer.render(&scene, &data(), 101).unwrap();
    let stale = renderer.render(&scene, &data(), 160).unwrap();
    assert_eq!(fresh.components[0].state, DataState::Fresh);
    assert_eq!(stale.components[0].state, DataState::Stale);
    for y in 0..74 {
        for x in 0..192 {
            assert_eq!(fresh.frame.is_black(x, y), stale.frame.is_black(x, y));
        }
    }
    assert_ne!(fresh.frame, stale.frame);
    let mut failed = data();
    failed.snapshots[0].last_error = Some("private diagnostic".into());
    assert_eq!(
        renderer.render(&scene, &failed, 101).unwrap().frame,
        stale.frame
    );
}

#[test]
fn binding_uses_connection_identity_and_not_just_dataset_name() {
    let scene = layout();
    let mut snapshots = data();
    let mut other = snapshots.snapshots[0].clone();
    other.connection_id = "work".into();
    other.dataset.rows[0].insert("temperature".into(), json!(99.0));
    snapshots.snapshots.push(other);
    let mut renderer = Renderer::new().unwrap();
    let before = renderer.render(&scene, &snapshots, 101).unwrap();
    snapshots.snapshots[1].dataset.rows[0].insert("temperature".into(), json!(12.3));
    assert_eq!(
        before.frame,
        renderer.render(&scene, &snapshots, 101).unwrap().frame
    );
    let mut changed = serde_json::to_value(scene).unwrap();
    changed["components"][0]["blocks"][0]["source"]["binding"]["connection_id"] = json!("work");
    let changed: Layout = serde_json::from_value(changed).unwrap();
    assert_ne!(
        before.frame,
        renderer.render(&changed, &snapshots, 101).unwrap().frame
    );
}

#[test]
fn absent_references_and_successful_empty_results_are_distinct() {
    let scene = layout();
    let mut renderer = Renderer::new().unwrap();
    let mut snapshots = data();
    snapshots.snapshots.clear();
    let result = renderer.render(&scene, &snapshots, 101).unwrap();
    assert_eq!(result.components[0].state, DataState::Unavailable);
    assert!(
        result.components[0]
            .warnings
            .contains(&RenderWarning::MissingDataset)
    );
    let mut snapshots = data();
    snapshots.snapshots[0].dataset.rows.clear();
    let result = renderer.render(&scene, &snapshots, 101).unwrap();
    assert_eq!(result.components[0].state, DataState::Fresh);
    assert!(result.components[0].warnings.is_empty());
    let mut snapshots = data();
    snapshots.snapshots[0].dataset.rows[0].insert("temperature".into(), json!(null));
    let result = renderer.render(&scene, &snapshots, 101).unwrap();
    assert_eq!(result.components[0].state, DataState::Unavailable);
    assert!(
        result.components[0]
            .warnings
            .contains(&RenderWarning::NullValue)
    );
}

#[test]
fn incompatible_formats_produce_a_component_warning() {
    let mut snapshots = data();
    snapshots.snapshots[0].dataset.fields[0].kind = paperphase_core::FieldKind::Boolean;
    snapshots.snapshots[0].dataset.rows[0].insert("temperature".into(), json!(true));
    let result = Renderer::new()
        .unwrap()
        .render(&layout(), &snapshots, 101)
        .unwrap();
    assert_eq!(result.components[0].state, DataState::Unavailable);
    assert!(
        result.components[0]
            .warnings
            .contains(&RenderWarning::IncompatibleFormat)
    );
}

#[test]
fn changes_include_the_previous_position_and_removed_content() {
    let mut scene = literal("Example", 20, "wrap");
    let mut renderer = Renderer::new().unwrap();
    let before = renderer.render(&scene, &data(), 101).unwrap();
    let identical = renderer.render(&scene, &data(), 101).unwrap();
    assert!(
        before
            .frame
            .difference_bounds(&identical.frame)
            .unwrap()
            .is_none()
    );
    scene.components[0].rect.x = 96;
    let after = renderer.render(&scene, &data(), 101).unwrap();
    let dirty = after
        .frame
        .difference_bounds(&before.frame)
        .unwrap()
        .unwrap();
    assert!(dirty.x < 96 && dirty.right() > 96);
    scene.components.clear();
    let empty = renderer.render(&scene, &data(), 101).unwrap();
    assert!(empty.frame.bits().iter().all(|byte| *byte == 0));
    assert!(
        empty
            .frame
            .difference_bounds(&after.frame)
            .unwrap()
            .is_some()
    );
    let other = Frame::blank(&paperphase_core::DisplaySpec {
        id: "other".into(),
        width: 16,
        height: 16,
    })
    .unwrap();
    assert!(empty.frame.difference_bounds(&other).is_err());
}

#[test]
fn wrapping_and_ellipsis_reflow_without_scaling_the_font() {
    let mut scene = literal("Words that wrap naturally across several lines", 20, "wrap");
    let mut renderer = Renderer::new().unwrap();
    let narrow = renderer.render(&scene, &data(), 101).unwrap();
    scene.components[0].rect.width = 192;
    let wide = renderer.render(&scene, &data(), 101).unwrap();
    let ink_below =
        |frame: &Frame| (32..96).any(|y| (0..96).any(|x| frame.is_black(x, y) == Some(true)));
    assert!(ink_below(&narrow.frame));
    assert_ne!(narrow.frame, wide.frame);
    let ellipsis = renderer
        .render(
            &literal(
                "Words that wrap naturally across several lines",
                20,
                "ellipsis",
            ),
            &data(),
            101,
        )
        .unwrap();
    assert!(
        ellipsis.components[0]
            .warnings
            .contains(&RenderWarning::Overflow)
    );
    assert!(!ink_below(&ellipsis.frame));
}

#[test]
fn unicode_fallback_is_reported_and_accents_are_preserved() {
    let mut renderer = Renderer::new().unwrap();
    let normal = renderer
        .render(&literal("Éireann · 16 °C", 16, "wrap"), &data(), 101)
        .unwrap();
    assert!(
        !normal.components[0]
            .warnings
            .contains(&RenderWarning::GlyphFallback)
    );
    let fallback = renderer
        .render(&literal("Missing: \u{10ffff}", 16, "wrap"), &data(), 101)
        .unwrap();
    assert!(
        fallback.components[0]
            .warnings
            .contains(&RenderWarning::GlyphFallback)
    );
}

#[test]
fn invalid_layout_is_rejected_before_rendering() {
    let mut scene = layout();
    let mut duplicate = scene.components[0].clone();
    duplicate.id = "second".into();
    scene.components.push(duplicate);
    let error = Renderer::new()
        .unwrap()
        .render(&scene, &data(), 101)
        .err()
        .unwrap();
    assert!(error.to_string().contains("overlaps"));
    assert!(
        Rect {
            x: 0,
            y: 0,
            width: 32,
            height: 32
        }
        .contains(31, 31)
    );
    assert!(
        !Rect {
            x: 0,
            y: 0,
            width: 32,
            height: 32
        }
        .contains(32, 31)
    );
}

#[test]
fn measured_flow_boxes_preserve_pixels_when_converted_to_positioned_elements() {
    use paperphase_core::{Align, ContentLayout};
    let mut renderer = Renderer::new().unwrap();
    for alignment in [Align::Left, Align::Center, Align::Right] {
        for at in [101, 160] {
            let mut scene = layout();
            scene.components[0].gap = 8;
            let mut title = scene.components[0].blocks[0].clone();
            title.id = "title".into();
            title.source = paperphase_core::TextSource::Literal {
                text: "Égy\nDublin".into(),
            };
            title.font_size = 12;
            title.align = alignment;
            scene.components[0].blocks.insert(0, title);
            let before = renderer.render(&scene, &data(), at).unwrap();
            scene.components[0].content_layout = ContentLayout::Positioned;
            for (block, report) in scene.components[0]
                .blocks
                .iter_mut()
                .zip(&before.components[0].blocks)
            {
                block.rect = report.rect;
            }
            let after = renderer.render(&scene, &data(), at).unwrap();
            assert_eq!(before.frame, after.frame);
            assert_eq!(before.components[0].state, after.components[0].state);
        }
    }
}

#[test]
fn positioned_text_is_clipped_to_its_box_and_moving_it_clears_old_pixels() {
    use paperphase_core::ContentLayout;
    let mut scene = literal("MMMMMMMM", 32, "wrap");
    scene.components[0].content_layout = ContentLayout::Positioned;
    let area = Rect {
        x: 8,
        y: 8,
        width: 40,
        height: 24,
    };
    scene.components[0].blocks[0].rect = Some(area);
    let mut renderer = Renderer::new().unwrap();
    let before = renderer.render(&scene, &data(), 101).unwrap();
    assert!(
        before.components[0]
            .warnings
            .contains(&RenderWarning::Overflow)
    );
    assert!(before.frame.bits().iter().any(|byte| *byte != 0));
    for y in 0..96 {
        for x in 0..192 {
            if !area.contains(x, y) {
                assert_eq!(before.frame.is_black(x as u32, y as u32), Some(false));
            }
        }
    }
    scene.components[0].blocks[0].rect.as_mut().unwrap().x = 56;
    let after = renderer.render(&scene, &data(), 101).unwrap();
    for y in 0..96 {
        for x in 0..56 {
            assert_eq!(after.frame.is_black(x, y), Some(false));
        }
    }
    let dirty = before
        .frame
        .difference_bounds(&after.frame)
        .unwrap()
        .unwrap();
    assert!(dirty.x < 48 && dirty.right() > 56);
    let first = scene.components[0].blocks[0].clone();
    let mut overlapping = first;
    overlapping.id = "overlapping".into();
    scene.components[0].blocks.push(overlapping);
    assert!(renderer.render(&scene, &data(), 101).is_err());
}

#[test]
fn flow_reports_identify_elements_hidden_below_the_content_area() {
    let mut scene = literal("A", 128, "clip");
    let mut hidden = scene.components[0].blocks[0].clone();
    hidden.id = "hidden".into();
    scene.components[0].blocks.push(hidden);
    let result = Renderer::new()
        .unwrap()
        .render(&scene, &data(), 101)
        .unwrap();
    assert!(result.components[0].blocks[0].rect.is_some());
    assert!(result.components[0].blocks[1].rect.is_none());
}

#[test]
fn local_weather_symbols_use_the_bundled_monochrome_font() {
    let result = Renderer::new()
        .unwrap()
        .render(&literal("☀ ☾ ☁ ≋ ☂ ❄ ⚡ ?", 24, "wrap"), &data(), 101)
        .unwrap();
    assert!(
        !result.components[0]
            .warnings
            .contains(&RenderWarning::GlyphFallback)
    );
}
