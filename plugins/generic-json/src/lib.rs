//! Explicit JSON Pointer mappings; no scripts, coercion or independent HTTP client.
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, &'static str>;
#[derive(Deserialize)]
struct Mapping {
    field: String,
    pointer: String,
    kind: String,
    #[serde(default)]
    unit: String,
}
#[derive(Deserialize)]
struct Settings {
    label: String,
    url: String,
    root_pointer: Option<String>,
    timestamp_pointer: Option<String>,
    max_rows: usize,
    mappings: Vec<Mapping>,
}
fn pointer_valid(pointer: &str) -> bool {
    if pointer.is_empty() {
        return true;
    }
    if !pointer.starts_with('/') || pointer.len() > 256 {
        return false;
    }
    let mut chars = pointer.chars();
    while let Some(ch) = chars.next() {
        if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}
fn settings(values: &SettingsValues) -> Result<Settings> {
    let s: Settings =
        serde_json::from_value(json!(values)).map_err(|_| "invalid JSON connector settings")?;
    let mut ids = BTreeSet::new();
    let url = url::Url::parse(&s.url).map_err(|_| "invalid JSON source URL")?;
    if s.label.trim().is_empty()
        || s.label.len() > 128
        || !(1..=20).contains(&s.max_rows)
        || s.mappings.is_empty()
        || s.mappings.len() > 16
        || !pointer_valid(s.root_pointer.as_deref().unwrap_or(""))
        || !pointer_valid(s.timestamp_pointer.as_deref().unwrap_or(""))
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || s.mappings.iter().any(|m| {
            !paperphase_plugin_sdk::valid_id(&m.field)
                || !ids.insert(&m.field)
                || !pointer_valid(&m.pointer)
                || m.unit.len() > 32
                || !matches!(m.kind.as_str(), "text" | "number" | "boolean")
        })
    {
        return Err("invalid JSON connector settings");
    }
    Ok(s)
}
pub fn convert(bytes: &[u8], values: &SettingsValues) -> Result<Publication> {
    if bytes.len() > 65536 {
        return Err("JSON response exceeds budget");
    }
    let s = settings(values)?;
    let document: Value = serde_json::from_slice(bytes).map_err(|_| "invalid source JSON")?;
    let observed = if s.timestamp_pointer.as_deref().unwrap_or("").is_empty() {
        None
    } else {
        Some(
            document
                .pointer(s.timestamp_pointer.as_deref().unwrap_or(""))
                .and_then(Value::as_u64)
                .filter(|v| *v > 0 && *v <= i64::MAX as u64)
                .ok_or("invalid observation timestamp")?,
        )
    };
    let root = document
        .pointer(s.root_pointer.as_deref().unwrap_or(""))
        .ok_or("JSON root pointer not found")?;
    let items = if let Some(array) = root.as_array() {
        array.iter().take(s.max_rows).collect::<Vec<_>>()
    } else {
        vec![root]
    };
    let mut rows = Vec::new();
    let mut summaries = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        let mut row = BTreeMap::new();
        let mut lines = Vec::new();
        for mapping in &s.mappings {
            let value = item
                .pointer(&mapping.pointer)
                .cloned()
                .unwrap_or(Value::Null);
            let valid = value.is_null()
                || match mapping.kind.as_str() {
                    "text" => value.as_str().is_some_and(|s| s.len() <= 1024),
                    "number" => value.as_f64().is_some_and(f64::is_finite),
                    "boolean" => value.is_boolean(),
                    _ => false,
                };
            if !valid {
                return Err("mapped JSON value has the wrong type or size");
            }
            let display = match &value {
                Value::Null => "Unavailable".into(),
                Value::String(s) => s.clone(),
                Value::Bool(v) => {
                    if *v {
                        "Yes".into()
                    } else {
                        "No".into()
                    }
                }
                _ => value.to_string(),
            };
            lines.push(format!(
                "{}: {}{}",
                mapping.field,
                display,
                if mapping.unit.is_empty() || value.is_null() {
                    String::new()
                } else {
                    format!(" {}", mapping.unit)
                }
            ));
            row.insert(mapping.field.clone(), value);
        }
        summaries.push(format!(
            "{}{}",
            if index == 0 {
                String::new()
            } else {
                format!("Record {}\n", index + 1)
            },
            lines.join("\n")
        ));
        rows.push(row);
    }
    let summary = if summaries.is_empty() {
        "No records".into()
    } else {
        summaries.join("\n\n")
    };
    if summary.len() > 4096 {
        return Err("JSON summary exceeds display budget; select fewer rows or fields");
    }
    let fields = s
        .mappings
        .iter()
        .map(|m| Field {
            id: m.field.clone(),
            kind: match m.kind.as_str() {
                "number" => FieldKind::Number,
                "boolean" => FieldKind::Boolean,
                _ => FieldKind::Text,
            },
            unit: (!m.unit.is_empty()).then(|| m.unit.clone()),
        })
        .collect();
    let publication = Publication {
        dataset: Dataset {
            schema_version: 1,
            id: "records".into(),
            fields,
            rows,
        },
        observed_at: observed,
        min_poll_secs: None,
        additional_datasets: vec![Dataset {
            schema_version: 1,
            id: "overview".into(),
            fields: ["title", "summary"]
                .map(|id| Field {
                    id: id.into(),
                    kind: FieldKind::Text,
                    unit: None,
                })
                .to_vec(),
            rows: vec![BTreeMap::from([
                ("title".into(), json!(s.label)),
                ("summary".into(), json!(summary)),
            ])],
        }],
        additional_observed_at: observed.map_or_else(BTreeMap::new, |at| {
            BTreeMap::from([("overview".into(), at)])
        }),
    };
    publication
        .validate()
        .map_err(|_| "invalid JSON publication")?;
    Ok(publication)
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    fn fetch() -> std::result::Result<(), ()> {
        let values = paperphase_plugin_sdk::settings().map_err(|_| ())?;
        let s = settings(&values).map_err(|_| ())?;
        let bytes = paperphase_plugin_sdk::http_get(&s.url).map_err(|_| ())?;
        let publication = convert(&bytes, &values).map_err(|_| ())?;
        paperphase_plugin_sdk::publish_with_metadata(&publication).map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn values() -> SettingsValues {
        serde_json::from_value(json!({"label":"Room sensor","url":"https://example.com/data","root_pointer":"/sensors","timestamp_pointer":"/observed_at","max_rows":3,"mappings":[{"field":"room","pointer":"/name","kind":"text","unit":""},{"field":"temperature","pointer":"/temperature","kind":"number","unit":"C"},{"field":"open","pointer":"/open","kind":"boolean","unit":""},{"field":"escaped","pointer":"/a~1b/~0key","kind":"text","unit":""}]})).unwrap()
    }
    #[test]
    fn maps_arrays_escaped_paths_nulls_zero_and_false_without_coercion() {
        let p = convert(include_bytes!("../fixtures/sensors.json"), &values()).unwrap();
        assert_eq!(p.dataset.rows.len(), 2);
        assert_eq!(p.dataset.rows[0]["temperature"], 0);
        assert_eq!(p.dataset.rows[0]["open"], false);
        assert_eq!(p.dataset.rows[0]["escaped"], "literal keys");
        assert!(p.dataset.rows[1]["temperature"].is_null());
        assert_eq!(p.additional_observed_at["overview"], 1700000000);
    }
    #[test]
    fn wrong_types_paths_timestamps_and_duplicate_fields_fail() {
        let mut document: Value =
            serde_json::from_slice(include_bytes!("../fixtures/sensors.json")).unwrap();
        document["sensors"][0]["temperature"] = json!("20");
        assert!(convert(&serde_json::to_vec(&document).unwrap(), &values()).is_err());
        for key in ["root_pointer", "timestamp_pointer"] {
            let mut v = values();
            v.insert(key.into(), json!("/not-found"));
            assert!(convert(include_bytes!("../fixtures/sensors.json"), &v).is_err());
        }
        let mut v = values();
        v.get_mut("mappings").unwrap()[1]["field"] = json!("room");
        assert!(convert(include_bytes!("../fixtures/sensors.json"), &v).is_err());
        assert!(!pointer_valid("/bad~2path"));
    }
    #[test]
    fn optional_null_pointers_clear_to_document_root_and_fetch_freshness() {
        let mut values = values();
        values.insert("root_pointer".into(), Value::Null);
        values.insert("timestamp_pointer".into(), Value::Null);
        values.insert(
            "mappings".into(),
            json!([{"field":"value","pointer":"/value","kind":"number","unit":""}]),
        );
        let p = convert(br#"{"value":7}"#, &values).unwrap();
        assert_eq!(p.dataset.rows[0]["value"], 7);
        assert_eq!(p.observed_at, None);
    }

    #[test]
    fn valid_empty_arrays_publish_empty_records_and_an_explanation() {
        let p = convert(br#"{"observed_at":1700000000,"sensors":[]}"#, &values()).unwrap();
        assert!(p.dataset.rows.is_empty());
        assert_eq!(p.additional_datasets[0].rows[0]["summary"], "No records");
    }
}
