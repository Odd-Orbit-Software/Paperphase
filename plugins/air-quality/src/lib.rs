//! Modelled air quality and pollen through one shared-host Open-Meteo request.
use paperphase_plugin_sdk::clock::{ClockContent, ClockSpec, DateFormat, HourCycle};
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Result<T> = std::result::Result<T, &'static str>;
const POLLEN: [(&str, &str); 6] = [
    ("grass_pollen", "Grass"),
    ("birch_pollen", "Birch"),
    ("alder_pollen", "Alder"),
    ("mugwort_pollen", "Mugwort"),
    ("olive_pollen", "Olive"),
    ("ragweed_pollen", "Ragweed"),
];
const FIELDS: [&str; 11] = [
    "european_aqi",
    "pm2_5",
    "pm10",
    "nitrogen_dioxide",
    "ozone",
    "grass_pollen",
    "birch_pollen",
    "alder_pollen",
    "mugwort_pollen",
    "olive_pollen",
    "ragweed_pollen",
];
const CREDIT: &str = "CAMS ENSEMBLE · open-meteo.com · CC BY 4.0";
#[derive(Deserialize)]
struct Settings {
    location: String,
    latitude: f64,
    longitude: f64,
    forecast_hours: usize,
    pollen_focus: String,
}
impl Settings {
    fn parse(values: &SettingsValues) -> Result<Self> {
        let s: Self =
            serde_json::from_value(json!(values)).map_err(|_| "invalid air-quality settings")?;
        if s.location.trim().is_empty()
            || s.location.len() > 128
            || !(-90.0..=90.0).contains(&s.latitude)
            || !(-180.0..=180.0).contains(&s.longitude)
            || !(6..=48).contains(&s.forecast_hours)
            || !(s.pollen_focus == "all" || POLLEN.iter().any(|(id, _)| *id == s.pollen_focus))
        {
            return Err("invalid air-quality settings");
        }
        Ok(s)
    }
}
pub fn request_url(source: &str, values: &SettingsValues) -> Result<String> {
    let s = Settings::parse(values)?;
    let mut url = url::Url::parse(source).map_err(|_| "invalid air-quality URL")?;
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("latitude", &s.latitude.to_string())
        .append_pair("longitude", &s.longitude.to_string())
        .append_pair("current", &FIELDS.join(","))
        .append_pair("hourly", &FIELDS.join(","))
        .append_pair("forecast_hours", &(s.forecast_hours + 1).to_string())
        .append_pair("timezone", "auto")
        .append_pair("timeformat", "unixtime");
    Ok(url.into())
}
#[derive(Deserialize)]
struct Sample {
    time: u64,
    #[serde(flatten)]
    values: BTreeMap<String, Value>,
}
#[derive(Deserialize)]
struct Hours {
    time: Vec<u64>,
    #[serde(flatten)]
    values: BTreeMap<String, Vec<Option<f64>>>,
}
#[derive(Deserialize)]
struct Response {
    timezone: String,
    current: Sample,
    current_units: BTreeMap<String, String>,
    hourly: Hours,
    hourly_units: BTreeMap<String, String>,
}
fn category(v: Option<f64>) -> &'static str {
    match v {
        None => "Unavailable",
        Some(v) if v <= 20.0 => "Good",
        Some(v) if v <= 40.0 => "Fair",
        Some(v) if v <= 60.0 => "Moderate",
        Some(v) if v <= 80.0 => "Poor",
        Some(v) if v <= 100.0 => "Very poor",
        Some(_) => "Extremely poor",
    }
}
fn value(sample: &Sample, key: &str) -> Result<Option<f64>> {
    match sample.values.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .map(Some)
            .ok_or("invalid concentration"),
    }
}
fn unit(units: &BTreeMap<String, String>, key: &str) -> bool {
    match units.get(key).map(String::as_str) {
        Some("EAQI") => key == "european_aqi",
        Some("μg/m³" | "µg/m³") => {
            matches!(key, "pm2_5" | "pm10" | "nitrogen_dioxide" | "ozone")
        }
        Some("grains/m³") => key.ends_with("_pollen"),
        _ => false,
    }
}
fn display(v: Option<f64>, suffix: &str) -> String {
    v.map_or_else(|| "Unavailable".into(), |v| format!("{v:.1}{suffix}"))
}
fn text(id: &str) -> Field {
    Field {
        id: id.into(),
        kind: FieldKind::Text,
        unit: None,
    }
}
fn number(id: &str, unit: &str) -> Field {
    Field {
        id: id.into(),
        kind: FieldKind::Number,
        unit: Some(unit.into()),
    }
}
fn dataset(id: &str, fields: Vec<Field>, rows: Vec<BTreeMap<String, Value>>) -> Dataset {
    Dataset {
        schema_version: 1,
        id: id.into(),
        fields,
        rows,
    }
}
pub fn convert(bytes: &[u8], values: &SettingsValues) -> Result<Publication> {
    if bytes.len() > 65536 {
        return Err("air-quality response exceeds budget");
    }
    let s = Settings::parse(values)?;
    let r: Response = serde_json::from_slice(bytes).map_err(|_| "invalid air-quality response")?;
    if r.current.time == 0
        || r.hourly.time.len() > 192
        || !r.hourly.time.windows(2).all(|w| w[0] < w[1])
        || r.hourly.values.values().any(|v| {
            v.len() != r.hourly.time.len() || v.iter().flatten().any(|n| !n.is_finite() || *n < 0.0)
        })
    {
        return Err("invalid air-quality forecast arrays");
    }
    for key in FIELDS {
        if r.current.values.get(key).is_some_and(|v| !v.is_null()) && !unit(&r.current_units, key)
            || r.hourly
                .values
                .get(key)
                .is_some_and(|v| v.iter().any(Option::is_some))
                && !unit(&r.hourly_units, key)
        {
            return Err("unexpected air-quality units");
        }
    }
    let clock = ClockSpec {
        timezone: r.timezone,
        content: ClockContent::Time,
        hour_cycle: HourCycle::TwentyFour,
        date_format: DateFormat::Iso,
    };
    clock
        .validate()
        .map_err(|_| "invalid air-quality timezone")?;
    let date_clock = ClockSpec {
        content: ClockContent::Date,
        ..clock.clone()
    };
    let today = date_clock
        .format(r.current.time)
        .map_err(|_| "invalid model date")?;
    let local_time = |at| -> Result<String> {
        let day = date_clock.format(at).map_err(|_| "invalid forecast date")?;
        let time = clock.format(at).map_err(|_| "invalid forecast time")?;
        Ok(if day == today {
            time
        } else {
            format!("{day} {time}")
        })
    };
    let selected = r
        .hourly
        .time
        .iter()
        .enumerate()
        .filter(|(_, at)| **at >= r.current.time)
        .take(s.forecast_hours)
        .collect::<Vec<_>>();
    let aqi = value(&r.current, "european_aqi")?;
    let mut pollen_rows = Vec::new();
    let mut pollen_lines = Vec::new();
    let mut outlook_lines = Vec::new();
    for (key, name) in POLLEN {
        let current = value(&r.current, key)?;
        let series = r.hourly.values.get(key);
        let peak = selected
            .iter()
            .filter_map(|(i, at)| series.and_then(|v| v[*i]).map(|v| (**at, v)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let complete = selected.len() == s.forecast_hours
            && selected
                .iter()
                .all(|(i, _)| series.is_some_and(|v| v[*i].is_some()));
        let peak_label = peak
            .map(|(at, v)| {
                local_time(at).map(|t| {
                    format!(
                        "{v:.1} at {t}{}",
                        if complete { "" } else { " (partial forecast)" }
                    )
                })
            })
            .transpose()
            .map_err(|_| "invalid pollen forecast time")?
            .unwrap_or_else(|| "Unavailable".into());
        pollen_rows.push(BTreeMap::from([
            ("name".into(), json!(name)),
            ("kind".into(), json!(key)),
            ("count".into(), json!(current)),
            (
                "display-count".into(),
                json!(display(current, " grains/m³")),
            ),
            ("peak-count".into(), json!(peak.map(|(_, v)| v))),
            ("peak-at".into(), json!(peak.map(|(at, _)| at))),
            ("peak-label".into(), json!(peak_label)),
            (
                "coverage".into(),
                json!(if complete {
                    "complete"
                } else {
                    "partial or unavailable"
                }),
            ),
        ]));
        if s.pollen_focus == "all" || s.pollen_focus == key {
            outlook_lines.push(format!("{name} peak: {peak_label}"));
            pollen_lines.push(format!("{name}: {}", display(current, " grains/m³")));
        }
    }
    let mut hourly_rows = Vec::new();
    for (i, at) in selected {
        let mut row = BTreeMap::from([
            ("time".into(), json!(at)),
            ("time-label".into(), json!(local_time(*at)?)),
        ]);
        for key in FIELDS {
            row.insert(
                key.replace('_', "-"),
                json!(r.hourly.values.get(key).and_then(|v| v[i])),
            );
        }
        row.insert(
            "category".into(),
            json!(category(
                r.hourly.values.get("european_aqi").and_then(|v| v[i])
            )),
        );
        hourly_rows.push(row);
    }
    let forecast = hourly_rows
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            hourly_rows.len() <= 6 || (0..6).any(|n| n * (hourly_rows.len() - 1) / 5 == *i)
        })
        .map(|(_, row)| {
            format!(
                "{} · AQI {} · {}",
                row["time-label"].as_str().unwrap_or(""),
                row["european-aqi"]
                    .as_f64()
                    .map_or_else(|| "Unavailable".into(), |v| format!("{v:.0}")),
                row["category"].as_str().unwrap_or("Unavailable")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut current = BTreeMap::from([
        ("location".into(), json!(s.location)),
        (
            "aqi-label".into(),
            json!(aqi.map_or_else(
                || "Unavailable".into(),
                |v| format!("{v:.0} · {}", category(Some(v)))
            )),
        ),
        ("category".into(), json!(category(aqi))),
        ("forecast-summary".into(), json!(forecast)),
        ("pollen-summary".into(), json!(pollen_lines.join("\n"))),
        ("pollen-outlook".into(), json!(outlook_lines.join("\n"))),
        (
            "updated-label".into(),
            json!(format!(
                "Model estimate {}",
                clock
                    .format(r.current.time)
                    .map_err(|_| "invalid model time")?
            )),
        ),
        ("attribution".into(), json!(CREDIT)),
    ]);
    for key in ["european_aqi", "pm2_5", "pm10", "nitrogen_dioxide", "ozone"] {
        current.insert(key.replace('_', "-"), json!(value(&r.current, key)?));
    }
    current.insert(
        "particles-label".into(),
        json!(format!(
            "PM2.5 {} · PM10 {}",
            display(value(&r.current, "pm2_5")?, ""),
            display(value(&r.current, "pm10")?, " µg/m³")
        )),
    );
    let mut fields = vec![
        text("location"),
        text("aqi-label"),
        text("category"),
        text("forecast-summary"),
        text("pollen-summary"),
        text("pollen-outlook"),
        text("particles-label"),
        text("updated-label"),
        text("attribution"),
    ];
    fields.extend([
        number("european-aqi", "EAQI"),
        number("pm2-5", "µg/m³"),
        number("pm10", "µg/m³"),
        number("nitrogen-dioxide", "µg/m³"),
        number("ozone", "µg/m³"),
    ]);
    let mut hour_fields = vec![
        number("time", "unix-seconds"),
        text("time-label"),
        text("category"),
    ];
    hour_fields.extend(FIELDS.into_iter().map(|key| {
        number(
            &key.replace('_', "-"),
            if key == "european_aqi" {
                "EAQI"
            } else if key.ends_with("_pollen") {
                "grains/m³"
            } else {
                "µg/m³"
            },
        )
    }));
    let publication = Publication {
        dataset: dataset("current", fields, vec![current]),
        observed_at: Some(r.current.time),
        min_poll_secs: Some(900),
        additional_observed_at: BTreeMap::from([
            ("hourly".into(), r.current.time),
            ("pollen".into(), r.current.time),
        ]),
        additional_datasets: vec![
            dataset("hourly", hour_fields, hourly_rows),
            dataset(
                "pollen",
                vec![
                    text("name"),
                    text("kind"),
                    number("count", "grains/m³"),
                    text("display-count"),
                    number("peak-count", "grains/m³"),
                    number("peak-at", "unix-seconds"),
                    text("peak-label"),
                    text("coverage"),
                ],
                pollen_rows,
            ),
        ],
    };
    publication
        .validate()
        .map_err(|_| "invalid air-quality publication")?;
    Ok(publication)
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    fn fetch() -> std::result::Result<(), ()> {
        let settings = paperphase_plugin_sdk::settings().map_err(|_| ())?;
        let source = paperphase_plugin_sdk::source_url().map_err(|_| ())?;
        let bytes =
            paperphase_plugin_sdk::http_get(&request_url(&source, &settings).map_err(|_| ())?)
                .map_err(|_| ())?;
        let publication = convert(&bytes, &settings).map_err(|_| ())?;
        paperphase_plugin_sdk::publish_with_metadata(&publication).map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> SettingsValues {
        serde_json::from_value(json!({"location":"Dublin","latitude":53.3498,"longitude":-6.2603,"forecast_hours":24,"pollen_focus":"all"})).unwrap()
    }
    #[test]
    fn current_forecast_and_seasonal_pollen_preserve_values_and_units() {
        let p = convert(include_bytes!("../fixtures/forecast.json"), &settings()).unwrap();
        assert_eq!(p.dataset.rows[0]["category"], "Fair");
        assert_eq!(p.dataset.rows[0]["european-aqi"].as_f64(), Some(31.0));
        assert_eq!(p.additional_datasets[0].rows.len(), 24);
        assert_eq!(p.additional_datasets[1].rows[0]["count"], 0.0);
        assert!(p.additional_datasets[1].rows[1]["count"].is_null());
        assert_eq!(p.additional_observed_at["pollen"], 1700000000);
    }
    #[test]
    fn malformed_arrays_units_negative_values_and_settings_fail() {
        let original: Value =
            serde_json::from_slice(include_bytes!("../fixtures/forecast.json")).unwrap();
        for mode in 0..4 {
            let mut v = original.clone();
            match mode {
                0 => v["hourly"]["pm10"] = json!([]),
                1 => v["current_units"]["pm10"] = json!("mg/m³"),
                2 => v["current"]["pm10"] = json!(-1),
                _ => v["hourly"]["time"][1] = v["hourly"]["time"][0].clone(),
            };
            assert!(convert(&serde_json::to_vec(&v).unwrap(), &settings()).is_err());
        }
        let mut s = settings();
        s.insert("latitude".into(), json!(91));
        assert!(request_url("https://example.com/", &s).is_err());
    }
    #[test]
    fn classification_and_request_are_bounded() {
        for (v, label) in [
            (20.0, "Good"),
            (20.1, "Fair"),
            (40.0, "Fair"),
            (60.0, "Moderate"),
            (80.0, "Poor"),
            (100.0, "Very poor"),
            (101.0, "Extremely poor"),
        ] {
            assert_eq!(category(Some(v)), label);
        }
        let url = request_url("https://example.com/air?old=1", &settings()).unwrap();
        assert!(url.contains("timeformat=unixtime"));
        assert!(url.contains("forecast_hours=25"));
        assert!(!url.contains("old=1"));
    }
}
