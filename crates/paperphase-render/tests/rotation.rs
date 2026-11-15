use paperphase_core::{Layout, SnapshotSet};
use paperphase_render::Renderer;
use std::collections::BTreeMap;

#[test]
fn selected_item_uses_the_shared_renderer_and_inactive_items_never_draw() {
    let layout: Layout =
        serde_json::from_str(include_str!("../../../examples/simulator/rotation.json")).unwrap();
    let data = SnapshotSet {
        schema_version: 1,
        snapshots: vec![],
    };
    let mut renderer = Renderer::new().unwrap();
    let first = renderer.render(&layout, &data, 0).unwrap();
    let selected = BTreeMap::from([("news".into(), "world".into())]);
    let second = renderer
        .render_selected(&layout, &data, 0, &selected)
        .unwrap();
    let flat = layout.resolve_rotations(&selected).unwrap();
    assert_eq!(
        second.frame,
        renderer.render(&flat, &data, 0).unwrap().frame
    );
    let changed = second
        .frame
        .difference_bounds(&first.frame)
        .unwrap()
        .unwrap();
    assert!(changed.x >= 8 && changed.y >= 8 && changed.right() <= 168 && changed.bottom() <= 128);
    assert_eq!(second.components.len(), 2);
    assert_eq!(second.components[0].id, "news");
    assert_eq!(second.components[0].blocks.len(), 2);
}
