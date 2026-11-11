use paperphase_core::{Layout, Rect, SnapshotSet};
use serde_json::json;

fn layout() -> Layout {
    serde_json::from_value(json!({
        "schema_version": 1,
        "display": {"id":"test","width":128,"height":64},
        "components": [{"id":"a","rect":{"x":0,"y":0,"width":64,"height":64},"padding":0,
            "blocks":[{"id":"text","font_size":16,"source":{"kind":"literal","text":"Example"}}]}]
    }))
    .unwrap()
}

fn snapshots() -> SnapshotSet {
    serde_json::from_value(json!({"schema_version":1,"snapshots":[{
        "connection_id":"weather.home","last_success_at":100,"stale_after_secs":60,
        "dataset":{"schema_version":1,"id":"current","fields":[{"id":"temperature","kind":"number"}],"rows":[{"temperature":18.5}]}
    }]})).unwrap()
}

#[test]
fn touching_edges_are_allowed_but_positive_area_overlap_is_rejected() {
    let mut value = layout();
    let mut neighbour = value.components[0].clone();
    neighbour.id = "b".into();
    neighbour.rect.x = 64;
    value.components.push(neighbour);
    value.validate().unwrap();
    value.components[1].rect.x = 63;
    let error = value.validate().unwrap_err();
    assert_eq!(error.path, "layout.components[1].rect");
    assert!(error.message.contains("overlaps"));
}

#[test]
fn geometry_is_checked_without_integer_wraparound() {
    let mut value = layout();
    value.components[0].rect.x = u32::MAX;
    assert!(value.validate().unwrap_err().message.contains("outside"));
    let wide = Rect {
        x: u32::MAX,
        y: 0,
        width: 64,
        height: 64,
    };
    assert!(wide.right() > u64::from(u32::MAX));
    assert!(!wide.intersects(Rect {
        x: 0,
        y: 0,
        width: 64,
        height: 64
    }));
    value.display.width = 4096;
    value.display.height = 4096;
    assert!(
        value
            .display
            .validate()
            .unwrap_err()
            .message
            .contains("budget")
    );
}

#[test]
fn identities_and_structure_survive_serialization_but_duplicates_fail() {
    let mut value = layout();
    value.components[0].blocks[0].id = "stable-title".into();
    value.components[0].label = Some("Kitchen weather".into());
    let roundtrip: Layout = serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(roundtrip.components[0].blocks[0].id, "stable-title");
    assert_eq!(
        roundtrip.components[0].label.as_deref(),
        Some("Kitchen weather")
    );
    value.components.push(value.components[0].clone());
    assert!(value.validate().unwrap_err().message.contains("duplicate"));
    let mut value = serde_json::to_value(layout()).unwrap();
    value["components"][0]["layer"] = json!(2);
    assert!(serde_json::from_value::<Layout>(value).is_err());
}

#[test]
fn binding_and_content_limits_return_field_paths() {
    let mut value = serde_json::to_value(layout()).unwrap();
    value["components"][0]["blocks"][0]["source"] = json!({"kind":"field","binding":{
        "connection_id":"weather.home","dataset_id":"current","field_id":"temperature","decimals":7
    }});
    let value: Layout = serde_json::from_value(value).unwrap();
    assert!(value.validate().unwrap_err().message.contains("precision"));
    let mut value = layout();
    value.components[0].blocks[0].font_size = 257;
    assert!(value.validate().unwrap_err().path.ends_with("font_size"));
    value.components[0].blocks[0].font_size = 16;
    value.components[0].padding = 64;
    assert!(value.validate().is_err());
}

#[test]
fn snapshot_keys_are_scoped_to_connections() {
    let mut value = snapshots();
    let mut other = value.snapshots[0].clone();
    other.connection_id = "weather.work".into();
    value.snapshots.push(other);
    value.validate().unwrap();
    value.snapshots[1].connection_id = "weather.home".into();
    assert!(
        value
            .validate()
            .unwrap_err()
            .message
            .contains("duplicate connection/dataset")
    );
}

#[test]
fn freshness_handles_deadlines_failures_and_clock_reversal() {
    let mut source = snapshots().snapshots.remove(0);
    assert!(!source.is_stale(159));
    assert!(source.is_stale(160));
    assert!(source.is_stale(99));
    source.observed_at = Some(50);
    assert!(source.is_stale(110));
    source.observed_at = Some(200);
    assert!(source.is_stale(101));
    source.observed_at = None;
    source.last_error = Some("source unavailable".into());
    assert!(source.is_stale(101));
    source.last_error = None;
    source.last_success_at = u64::MAX - 1;
    source.stale_after_secs = 60;
    assert!(!source.is_stale(u64::MAX));
}

#[test]
fn snapshot_dataset_validation_preserves_source_provenance() {
    let mut value = snapshots();
    value.snapshots[0].dataset.rows[0].insert("temperature".into(), json!("wrong type"));
    assert_eq!(
        value.validate().unwrap_err().path,
        "snapshots[0].dataset.rows[0].temperature"
    );
}

#[test]
fn positioned_elements_require_explicit_non_overlapping_content_local_boxes() {
    use paperphase_core::ContentLayout;
    let mut scene = layout();
    assert_eq!(scene.components[0].content_layout, ContentLayout::Flow);
    scene.components[0].content_layout = ContentLayout::Positioned;
    assert!(scene.validate().unwrap_err().message.contains("require"));
    scene.components[0].blocks[0].rect = Some(Rect {
        x: 0,
        y: 0,
        width: 32,
        height: 32,
    });
    let mut second = scene.components[0].blocks[0].clone();
    second.id = "second".into();
    second.rect.as_mut().unwrap().x = 32;
    scene.components[0].blocks.push(second);
    scene.validate().unwrap();
    let saved: Layout = serde_json::from_slice(&serde_json::to_vec(&scene).unwrap()).unwrap();
    assert_eq!(saved.components[0].blocks[1].rect.unwrap().x, 32);
    scene.components[0].blocks[1].rect.as_mut().unwrap().x = 31;
    assert!(scene.validate().unwrap_err().message.contains("overlaps"));
    scene.components[0].blocks.pop();
    scene.components[0].blocks[0].rect.as_mut().unwrap().width = 7;
    assert!(scene.validate().unwrap_err().message.contains("8 × 8"));
    scene.components[0].blocks[0].rect.as_mut().unwrap().width = 32;
    scene.components[0].blocks[0].rect.as_mut().unwrap().x = u32::MAX;
    assert!(scene.validate().unwrap_err().message.contains("outside"));
    scene.components[0].content_layout = ContentLayout::Flow;
    assert!(
        scene
            .validate()
            .unwrap_err()
            .message
            .contains("flow elements")
    );
}

#[test]
fn element_bounds_follow_padding_size_and_the_host_status_reservation() {
    let mut value = serde_json::to_value(layout()).unwrap();
    value["components"][0]["content_layout"] = json!("positioned");
    value["components"][0]["padding"] = json!(4);
    value["components"][0]["blocks"][0]["rect"] = json!({"x":0,"y":0,"width":56,"height":56});
    let literal: Layout = serde_json::from_value(value.clone()).unwrap();
    literal.validate().unwrap();
    value["components"][0]["blocks"][0]["source"] = json!({"kind":"field","binding":{
        "connection_id":"home","dataset_id":"current","field_id":"temperature"
    }});
    let mut bound: Layout = serde_json::from_value(value).unwrap();
    assert_eq!(bound.components[0].content_bounds().height, 38);
    assert!(bound.validate().unwrap_err().message.contains("outside"));
    bound.components[0].blocks[0].rect.as_mut().unwrap().height = 38;
    bound.validate().unwrap();
    bound.components[0].padding = 5;
    assert!(bound.validate().is_err());
    bound.components[0].padding = 4;
    bound.components[0].rect.width -= 1;
    assert!(bound.validate().is_err());
}
