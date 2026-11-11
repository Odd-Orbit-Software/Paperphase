//! Example connector used to validate the experimental plugin boundary.

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    match fetch() {
        Ok(()) => 0,
        Err(()) => 1,
    }
}

#[cfg(target_arch = "wasm32")]
fn fetch() -> Result<(), ()> {
    use paperphase_plugin_sdk::{
        Dataset, Field, FieldKind, http_get, publish_dataset, settings, source_url,
    };
    use serde_json::{Value, json};

    let source = source_url().map_err(|_| ())?;
    let response = http_get(&source).map_err(|_| ())?;
    let observation: Value = serde_json::from_slice(&response).map_err(|_| ())?;
    let settings = settings().map_err(|_| ())?;
    let location = settings
        .get("location")
        .and_then(Value::as_str)
        .unwrap_or("Observation");
    let imperial = settings.get("units").and_then(Value::as_str) == Some("imperial");
    let value = observation.get("temperature_c").ok_or(())?;
    let rows = if value.is_null() {
        Vec::new()
    } else {
        let temperature = value.as_f64().ok_or(())?;
        let display = if imperial {
            format!("{:.1} °F", temperature * 1.8 + 32.0)
        } else {
            format!("{temperature:.1} °C")
        };
        vec![std::collections::BTreeMap::from([
            ("temperature".into(), json!(temperature)),
            ("display-temperature".into(), json!(display)),
            ("location".into(), json!(location)),
        ])]
    };
    let dataset = Dataset {
        schema_version: 1,
        id: "current-weather".into(),
        fields: vec![
            Field {
                id: "temperature".into(),
                kind: FieldKind::Number,
                unit: Some("celsius".into()),
            },
            Field {
                id: "display-temperature".into(),
                kind: FieldKind::Text,
                unit: None,
            },
            Field {
                id: "location".into(),
                kind: FieldKind::Text,
                unit: None,
            },
        ],
        rows,
    };
    publish_dataset(&dataset).map_err(|_| ())
}
