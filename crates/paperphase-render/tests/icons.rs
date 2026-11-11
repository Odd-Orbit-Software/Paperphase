use paperphase_core::{Layout, SnapshotSet};
use paperphase_render::{DataState, IconStore, RenderWarning, Renderer};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn pack(root: &Path) -> Value {
    // Four samples repeated per row: opaque black, transparent black, white,
    // and sub-threshold alpha. Only the first may become ink.
    let pixels: Vec<u8> = (0..64)
        .flat_map(|i| match i % 8 {
            0 => [0, 0, 0, 255],
            1 => [0, 0, 0, 0],
            2 => [255, 255, 255, 255],
            3 => [0, 0, 0, 127],
            _ => [0, 0, 0, 0],
        })
        .collect();
    let mut encoder = png::Encoder::new(fs::File::create(root.join("sheet.png")).unwrap(), 8, 8);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    let manifest = json!({"schema_version":1,"id":"test.pack","name":"Test","variants":[{"size":8,"image":"sheet.png"}],"icons":{"sample":{"column":0,"row":0}}});
    save(root, &manifest);
    manifest
}
fn save(root: &Path, manifest: &Value) {
    fs::write(
        root.join("icons.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
}
fn scene(source: Value) -> Layout {
    serde_json::from_value(json!({"schema_version":1,"display":{"id":"test","width":96,"height":64},"components":[{"id":"icon","rect":{"x":0,"y":0,"width":64,"height":64},"padding":0,"blocks":[{"id":"glyph","source":source,"align":"right"}]}]})).unwrap()
}
fn source() -> Value {
    json!({"kind":"icon","pack_id":"test.pack","size":16,"icon":{"kind":"literal","icon_id":"sample"}})
}
fn empty() -> SnapshotSet {
    serde_json::from_value(json!({"schema_version":1,"snapshots":[]})).unwrap()
}

#[test]
fn generic_pack_alpha_scaling_alignment_clipping_and_missing_assets() {
    let dir = tempfile::tempdir().unwrap();
    pack(dir.path());
    let icons = IconStore::load_pack(&dir.path().join("icons.json"), "test.pack").unwrap();
    assert_eq!(icons.catalog()[0].sizes, vec![8]);
    let mut renderer = Renderer::with_icons(icons).unwrap();
    let rendered = renderer.render(&scene(source()), &empty(), 100).unwrap();
    assert!(rendered.components[0].warnings.is_empty());
    for y in 0..64 {
        for x in 0..96 {
            assert_eq!(
                rendered.frame.is_black(x, y),
                Some(y < 16 && (48..50).contains(&x))
            );
        }
    }
    let mut large = source();
    large["size"] = json!(128);
    let clipped = renderer.render(&scene(large), &empty(), 100).unwrap();
    assert!(
        clipped.components[0]
            .warnings
            .contains(&RenderWarning::Overflow)
    );
    assert!((0..64).all(|y| (64..96).all(|x| clipped.frame.is_black(x, y) == Some(false))));
    let missing = Renderer::new()
        .unwrap()
        .render(&scene(source()), &empty(), 100)
        .unwrap();
    assert!(
        missing.components[0]
            .warnings
            .contains(&RenderWarning::MissingIcon)
    );
}

#[test]
fn bound_icons_preserve_stale_pixels_reserve_status_and_allow_empty_data() {
    let dir = tempfile::tempdir().unwrap();
    pack(dir.path());
    let mut renderer = Renderer::with_icons(
        IconStore::load_pack(&dir.path().join("icons.json"), "test.pack").unwrap(),
    )
    .unwrap();
    let mut src = source();
    src["icon"] = json!({"kind":"field","binding":{"connection_id":"home","dataset_id":"current","field_id":"icon-key"}});
    let layout = scene(src.clone());
    assert!(layout.components[0].has_bindings());
    let mut data: SnapshotSet = serde_json::from_value(json!({"schema_version":1,"snapshots":[{"connection_id":"home","last_success_at":100,"stale_after_secs":60,"dataset":{"schema_version":1,"id":"current","fields":[{"id":"icon-key","kind":"text"}],"rows":[{"icon-key":"sample"}]}}]})).unwrap();
    let fresh = renderer.render(&layout, &data, 101).unwrap();
    let stale = renderer.render(&layout, &data, 160).unwrap();
    assert_eq!(fresh.components[0].state, DataState::Fresh);
    assert_eq!(stale.components[0].state, DataState::Stale);
    for y in 0..46 {
        for x in 0..96 {
            assert_eq!(fresh.frame.is_black(x, y), stale.frame.is_black(x, y));
        }
    }
    assert_ne!(fresh.frame, stale.frame);
    data.snapshots[0].dataset.rows.clear();
    let blank = renderer.render(&layout, &data, 101).unwrap();
    assert!(blank.components[0].warnings.is_empty());
    assert!(blank.frame.bits().iter().all(|byte| *byte == 0));
    src["icon"]["binding"]["prefix"] = json!("invalid");
    assert!(scene(src).validate().is_err());
}

#[test]
fn manifests_reject_escape_identity_invalid_cells_duplicate_sizes_and_merges() {
    let dir = tempfile::tempdir().unwrap();
    let original = pack(dir.path());
    assert!(IconStore::load_pack(&dir.path().join("icons.json"), "other").is_err());
    for (pointer, value) in [
        ("/variants/0/image", json!("../sheet.png")),
        ("/icons/sample/column", json!(1)),
        ("/icons/sample/row", json!(1)),
        ("/variants/0/size", json!(257)),
        ("/schema_version", json!(2)),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        save(dir.path(), &bad);
        assert!(
            IconStore::load_pack(&dir.path().join("icons.json"), "test.pack").is_err(),
            "{pointer}"
        );
    }
    let mut duplicate = original.clone();
    duplicate["variants"]
        .as_array_mut()
        .unwrap()
        .push(original["variants"][0].clone());
    save(dir.path(), &duplicate);
    assert!(IconStore::load_pack(&dir.path().join("icons.json"), "test.pack").is_err());
    save(dir.path(), &original);
    let mut icons = IconStore::load_pack(&dir.path().join("icons.json"), "test.pack").unwrap();
    assert!(icons.merge(icons.clone()).is_err());
    #[cfg(unix)]
    {
        let external = tempfile::tempdir().unwrap();
        fs::copy(
            dir.path().join("sheet.png"),
            external.path().join("outside.png"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            external.path().join("outside.png"),
            dir.path().join("link.png"),
        )
        .unwrap();
        let mut bad = original;
        bad["variants"][0]["image"] = json!("link.png");
        save(dir.path(), &bad);
        assert!(IconStore::load_pack(&dir.path().join("icons.json"), "test.pack").is_err());
    }
}

#[test]
fn bundled_weather_assets_load_without_weather_code_in_the_renderer() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/weather/assets/icons.json");
    let icons = IconStore::load_pack(&path, "weather.open-meteo").unwrap();
    assert_eq!(icons.catalog()[0].icons.len(), 33);
    assert_eq!(icons.catalog()[0].sizes, vec![16, 24, 32, 48, 64]);
}
