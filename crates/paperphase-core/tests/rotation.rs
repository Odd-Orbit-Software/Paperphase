use paperphase_core::{Layout, RefreshHint};
use std::collections::BTreeMap;

fn scene() -> Layout {
    serde_json::from_str(include_str!("../../../examples/simulator/rotation.json")).unwrap()
}

#[test]
fn one_area_resolves_one_independent_item_without_moving_pinned_content() {
    let mut layout = scene();
    layout.validate().unwrap();
    layout.components[0].refresh_hint = RefreshHint::Partial;
    let selected = BTreeMap::from([("news".into(), "world".into())]);
    let resolved = layout.resolve_rotations(&selected).unwrap();
    resolved.validate().unwrap();
    let item = &resolved.components[0];
    assert_eq!(item.id, "news");
    assert_eq!(item.rect.x, 8);
    assert_eq!(item.rect.y, 8);
    assert_eq!(item.rect.width, 128);
    assert_eq!(item.rect.height, 96);
    assert!(item.rotation.is_none());
    assert_eq!(item.refresh_hint, RefreshHint::Partial);
    assert_eq!(resolved.components[1].rect, layout.components[1].rect);
    assert!(layout.components[0].rotation.is_some());
    layout.components[0].rotation.as_mut().unwrap().members[1].refresh_hint = RefreshHint::Full;
    assert_eq!(
        layout.resolve_rotations(&selected).unwrap().components[0].refresh_hint,
        RefreshHint::Full
    );
}

#[test]
fn inactive_items_are_validated_and_cannot_escape_the_shared_rectangle() {
    let mut layout = scene();
    layout.components[0].rotation.as_mut().unwrap().members[1]
        .rect
        .width = 161;
    assert!(layout.validate().is_err());
    let mut layout = scene();
    layout.components[0].rotation.as_mut().unwrap().members[2].blocks[0].font_size = 1;
    assert!(layout.resolve_rotations(&BTreeMap::new()).is_err());
    let mut layout = scene();
    let nested = layout.components[0].rotation.clone();
    layout.components[0].rotation.as_mut().unwrap().members[0].rotation = nested;
    assert!(layout.validate().unwrap_err().message.contains("nested"));
    let mut layout = scene();
    layout.components[0]
        .rotation
        .as_mut()
        .unwrap()
        .paused_member = Some("missing".into());
    assert!(layout.validate().is_err());
}

#[test]
fn area_owns_space_and_all_inactive_text_counts_towards_the_budget() {
    let mut layout = scene();
    layout.components[1].rect.x = 144; // overlaps reserved area, outside the smaller member
    assert!(layout.validate().is_err());
    let mut layout = scene();
    for item in &mut layout.components[0].rotation.as_mut().unwrap().members {
        let mut block = item.blocks[0].clone();
        block.source = paperphase_core::TextSource::Literal {
            text: "x".repeat(4096),
        };
        item.blocks = (0..8)
            .map(|index| {
                let mut block = block.clone();
                block.id = format!("text-{index}");
                block
            })
            .collect();
    }
    assert!(
        layout
            .validate()
            .unwrap_err()
            .message
            .contains("text byte limit")
    );
    let layout = scene();
    assert!(
        layout
            .resolve_rotations(&BTreeMap::from([("pinned".into(), "world".into())]))
            .is_err()
    );
}
