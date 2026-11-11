use paperphase_core::RefreshSettings;
use serde_json::json;

#[test]
fn old_settings_gain_a_valid_policy_and_invalid_thresholds_are_rejected() {
    let settings: RefreshSettings = serde_json::from_value(json!({"quiet_hours":null})).unwrap();
    settings.validate().unwrap();
    assert_eq!(settings.policy.min_interval_ms, 250);
    for (field, value) in [
        ("min_interval_ms", 3600001),
        ("full_after_partials", 0),
        ("full_after_partials", 1001),
        ("full_after_secs", 0),
        ("full_after_secs", 604801),
        ("max_partial_area_percent", 0),
        ("max_partial_area_percent", 101),
    ] {
        let mut changed = serde_json::to_value(&settings).unwrap();
        changed["policy"][field] = json!(value);
        let invalid: RefreshSettings = serde_json::from_value(changed).unwrap();
        assert!(invalid.validate().unwrap_err().path.ends_with(field));
    }
}
