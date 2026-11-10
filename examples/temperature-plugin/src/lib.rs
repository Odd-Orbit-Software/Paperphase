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
    use paperphase_plugin_sdk::{http_get, publish, source_url};
    use serde_json::{Value, json};

    let source = source_url().map_err(|_| ())?;
    let response = http_get(&source).map_err(|_| ())?;
    let observation: Value = serde_json::from_slice(&response).map_err(|_| ())?;
    let temperature = observation
        .get("temperature_c")
        .and_then(Value::as_f64)
        .ok_or(())?;
    let dataset = json!({
        "schema_version": 1,
        "id": "current-weather",
        "fields": [{"id": "temperature", "kind": "number", "unit": "celsius"}],
        "rows": [{"temperature": temperature}]
    });
    publish(&serde_json::to_vec(&dataset).map_err(|_| ())?).map_err(|_| ())
}
