use paperphase_core::{ConnectorCatalog, SettingsValues};
use serde_json::json;

fn catalog() -> ConnectorCatalog {
    serde_json::from_str(include_str!("../../../examples/simulator/connectors.json")).unwrap()
}
fn values(value: serde_json::Value) -> SettingsValues {
    serde_json::from_value(value).unwrap()
}

#[test]
fn defaults_types_optional_values_and_polling_bounds_are_authoritative() {
    let catalog = catalog();
    catalog.validate().unwrap();
    let definition = &catalog.connectors[0];
    let defaults = definition.resolve(&SettingsValues::new(), None).unwrap();
    assert_eq!(defaults["location"], "Dublin");
    assert_eq!(defaults["forecast_days"], 3);
    assert_eq!(defaults["include_summary"], true);
    let settings = definition
        .resolve(
            &values(json!({"include_summary":false,"forecast_days":null,"units":"imperial"})),
            None,
        )
        .unwrap();
    assert_eq!(settings["include_summary"], false);
    assert_eq!(settings["forecast_days"], serde_json::Value::Null);
    for invalid in [
        json!({"unknown":true}),
        json!({"forecast_days":1.5}),
        json!({"forecast_days":8}),
        json!({"include_summary":"true"}),
        json!({"units":"kelvin"}),
        json!({"location":null}),
        json!({"location":" "}),
    ] {
        assert!(definition.resolve(&values(invalid), None).is_err());
    }
    let mut config: paperphase_core::ConnectionConfig = serde_json::from_value(json!({"id":"home","connector_id":"demo.weather","label":"Home","enabled":true,"poll_interval_secs":30,"settings":defaults})).unwrap();
    config.validate(&catalog).unwrap();
    config.poll_interval_secs = 29;
    assert!(config.validate(&catalog).is_err());
    config.poll_interval_secs = 30;
    config.label = String::new();
    assert!(config.validate(&catalog).is_err());
}

#[test]
fn omitted_secrets_retain_null_clears_and_errors_never_quote_submitted_values() {
    let catalog = catalog();
    let definition = &catalog.connectors[0];
    let original = definition
        .resolve(&values(json!({"test_secret":"private-token"})), None)
        .unwrap();
    assert_eq!(
        definition
            .resolve(&SettingsValues::new(), Some(&original))
            .unwrap()["test_secret"],
        "private-token"
    );
    assert!(
        !definition
            .resolve(&values(json!({"test_secret":null})), Some(&original))
            .unwrap()
            .contains_key("test_secret")
    );
    assert_eq!(
        definition
            .resolve(
                &values(json!({"test_secret":"replacement"})),
                Some(&original)
            )
            .unwrap()["test_secret"],
        "replacement"
    );
    let error = definition
        .resolve(&values(json!({"test_secret":["never-echo-this"]})), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("settings.test_secret"));
    assert!(!error.contains("never-echo-this"));
    let mut required = definition.clone();
    required.settings.last_mut().unwrap().required = true;
    assert!(required.resolve(&SettingsValues::new(), None).is_err());
    assert!(
        required
            .resolve(&values(json!({"test_secret":null})), Some(&original))
            .is_err()
    );
    assert!(
        required
            .resolve(&SettingsValues::new(), Some(&original))
            .is_ok()
    );
}

#[test]
fn publisher_definitions_are_bounded_unique_and_validate_defaults() {
    let base = serde_json::to_value(catalog()).unwrap();
    let mut cases = Vec::new();
    let mut changed = base.clone();
    changed["schema_version"] = json!(2);
    cases.push(changed);
    let mut changed = base.clone();
    changed["connectors"][0]["settings"][0]["input"]["max_bytes"] = json!(0);
    cases.push(changed);
    let mut changed = base.clone();
    changed["connectors"][0]["settings"][2]["input"]["default"] = json!(20);
    cases.push(changed);
    let mut changed = base.clone();
    changed["connectors"][0]["settings"][1]["input"]["options"][1]["value"] = json!("metric");
    cases.push(changed);
    let mut changed = base.clone();
    changed["connectors"][0]["settings"][1]["id"] = json!("location");
    cases.push(changed);
    let mut changed = base.clone();
    changed["connectors"][0]["min_poll_secs"] = json!(301);
    cases.push(changed);
    let mut changed = base.clone();
    changed["connectors"][0]["settings"][2]["input"]["max"] = json!(9_007_199_254_740_992_i64);
    cases.push(changed);
    for value in cases {
        let invalid: ConnectorCatalog = serde_json::from_value(value).unwrap();
        assert!(invalid.validate().is_err());
    }
    let mut secret_default = base;
    secret_default["connectors"][0]["settings"][4]["input"]["default"] =
        json!("must-not-be-public");
    assert!(serde_json::from_value::<ConnectorCatalog>(secret_default).is_err());
}

#[test]
fn unicode_limits_count_bytes_and_combined_values_are_bounded() {
    let catalog = catalog();
    let definition = &catalog.connectors[0];
    assert!(
        definition
            .resolve(&values(json!({"location":"é".repeat(64)})), None)
            .is_ok()
    );
    assert!(
        definition
            .resolve(&values(json!({"location":"é".repeat(65)})), None)
            .is_err()
    );
    let mut large = definition.clone();
    large.settings.clear();
    for i in 0..8 {
        large.settings.push(paperphase_core::SettingDefinition {
            id: format!("text-{i}"),
            label: format!("Text {i}"),
            help: String::new(),
            required: false,
            input: paperphase_core::SettingInput::Text {
                max_bytes: 4096,
                multiline: false,
                default: Some("x".repeat(4096)),
            },
        });
    }
    assert!(large.resolve(&SettingsValues::new(), None).is_err());
}

#[test]
fn plugin_decimal_urls_and_templates_validate_before_any_network_work() {
    let catalog = catalog();
    catalog.validate().unwrap();
    let weather = catalog.get("weather.open-meteo").unwrap();
    let resolved = weather
        .resolve(&values(json!({"latitude":53.3,"longitude":-6.2})), None)
        .unwrap();
    assert_eq!(resolved["latitude"], 53.3);
    assert!(
        weather
            .resolve(&values(json!({"latitude":91})), None)
            .is_err()
    );
    assert!(
        weather
            .resolve(&values(json!({"latitude":"53.3"})), None)
            .is_err()
    );
    let rss = catalog.get("rss.headlines").unwrap();
    for value in [
        "relative.xml",
        "file:///tmp/feed",
        "https://user:secret@example.org/feed",
        "https://example.org/feed#part",
    ] {
        assert!(
            rss.resolve(&values(json!({"feed_url":value})), None)
                .is_err()
        );
    }
    rss.resolve(
        &values(json!({"feed_url":"https://example.org/feed.xml"})),
        None,
    )
    .unwrap();
    let mut broken = catalog.clone();
    broken.connectors.iter_mut().find(|c| c.id == "weather.open-meteo").unwrap().templates[0].component.blocks[0].source = serde_json::from_value(json!({"kind":"field","binding":{"connection_id":"wrong","dataset_id":"current-weather","field_id":"location"}})).unwrap();
    assert!(broken.validate().is_err());
}
