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
    use paperphase_plugin_sdk::{Dataset, Field, FieldKind, http_get, publish_dataset, source_url};
    use serde_json::{Value, json};

    let source = source_url().map_err(|_| ())?;
    let response = http_get(&source).map_err(|_| ())?;
    let observation: Value = serde_json::from_slice(&response).map_err(|_| ())?;
    let temperature = observation
        .get("temperature_c")
        .and_then(Value::as_f64)
        .ok_or(())?;
    let dataset = Dataset {
        schema_version: 1,
        id: "current-weather".into(),
        fields: vec![Field {
            id: "temperature".into(),
            kind: FieldKind::Number,
            unit: Some("celsius".into()),
        }],
        rows: vec![std::collections::BTreeMap::from([(
            "temperature".into(),
            json!(temperature),
        )])],
    };
    publish_dataset(&dataset).map_err(|_| ())
}
