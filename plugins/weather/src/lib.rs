//! Open-Meteo current, hourly and daily data; HTTP remains a host service.
use paperphase_plugin_sdk::clock::{ClockContent, ClockSpec, DateFormat, HourCycle};
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, &'static str>;

#[derive(Deserialize)]
struct Settings {
    location: String,
    latitude: f64,
    longitude: f64,
    units: String,
    forecast_hours: usize,
    forecast_days: usize,
    rain_threshold: u32,
}
impl Settings {
    fn parse(values: &SettingsValues) -> Result<Self> {
        let value: Self = serde_json::from_value(json!(values)).map_err(|_| "invalid settings")?;
        if value.location.trim().is_empty()
            || value.location.len() > 128
            || !(-90.0..=90.0).contains(&value.latitude)
            || !(-180.0..=180.0).contains(&value.longitude)
            || !matches!(value.units.as_str(), "metric" | "imperial")
            || !(6..=48).contains(&value.forecast_hours)
            || !(1..=7).contains(&value.forecast_days)
            || !(1..=100).contains(&value.rain_threshold)
        {
            return Err("invalid settings");
        }
        Ok(value)
    }
}

pub fn request_url(source: &str, values: &SettingsValues) -> Result<String> {
    let settings = Settings::parse(values)?;
    let mut url = url::Url::parse(source).map_err(|_| "invalid source")?;
    url.set_query(None);
    url.query_pairs_mut().append_pair("latitude", &settings.latitude.to_string()).append_pair("longitude", &settings.longitude.to_string())
        .append_pair("current", "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,is_day,wind_speed_10m")
        .append_pair("hourly", "temperature_2m,rain,showers,precipitation_probability,weather_code,is_day")
        .append_pair("daily", "temperature_2m_min,temperature_2m_max,precipitation_sum,precipitation_probability_max,weather_code")
        .append_pair("forecast_hours", &(settings.forecast_hours + 1).to_string()).append_pair("forecast_days", &settings.forecast_days.to_string())
        .append_pair("temperature_unit", "celsius").append_pair("wind_speed_unit", "kmh").append_pair("precipitation_unit", "mm")
        .append_pair("timezone", "auto").append_pair("timeformat", "unixtime");
    Ok(url.into())
}

#[derive(Deserialize)]
struct Response {
    timezone: String,
    current: Current,
    current_units: Units,
    hourly: Hourly,
    daily: Daily,
}
#[derive(Deserialize)]
struct Units {
    temperature_2m: String,
    wind_speed_10m: String,
}
#[derive(Deserialize)]
struct Current {
    time: u64,
    temperature_2m: Option<f64>,
    apparent_temperature: Option<f64>,
    relative_humidity_2m: Option<f64>,
    weather_code: Option<u32>,
    is_day: Option<u8>,
    wind_speed_10m: Option<f64>,
}
#[derive(Deserialize)]
struct Hourly {
    is_day: Vec<Option<u8>>,
    time: Vec<u64>,
    temperature_2m: Vec<Option<f64>>,
    rain: Vec<Option<f64>>,
    showers: Vec<Option<f64>>,
    precipitation_probability: Vec<Option<f64>>,
    weather_code: Vec<Option<u32>>,
}
#[derive(Deserialize)]
struct Daily {
    time: Vec<u64>,
    temperature_2m_min: Vec<Option<f64>>,
    temperature_2m_max: Vec<Option<f64>>,
    precipitation_sum: Vec<Option<f64>>,
    precipitation_probability_max: Vec<Option<f64>>,
    weather_code: Vec<Option<u32>>,
}

pub fn convert(bytes: &[u8], values: &SettingsValues) -> Result<Publication> {
    let settings = Settings::parse(values)?;
    let response: Response =
        serde_json::from_slice(bytes).map_err(|_| "invalid weather response")?;
    if response.current_units.temperature_2m != "°C"
        || response.current_units.wind_speed_10m != "km/h"
    {
        return Err("unexpected units");
    }
    let h = &response.hourly;
    let d = &response.daily;
    if !same_lengths(
        h.time.len(),
        &[
            h.temperature_2m.len(),
            h.rain.len(),
            h.showers.len(),
            h.precipitation_probability.len(),
            h.weather_code.len(),
            h.is_day.len(),
        ],
    ) || !same_lengths(
        d.time.len(),
        &[
            d.temperature_2m_min.len(),
            d.temperature_2m_max.len(),
            d.precipitation_sum.len(),
            d.precipitation_probability_max.len(),
            d.weather_code.len(),
        ],
    ) || h.time.len() > 192
        || d.time.len() > 7
        || !ascending(&h.time)
        || !ascending(&d.time)
    {
        return Err("invalid forecast arrays");
    }
    let clock = ClockSpec {
        timezone: response.timezone.clone(),
        content: ClockContent::Time,
        hour_cycle: HourCycle::TwentyFour,
        date_format: DateFormat::Iso,
    };
    clock.validate().map_err(|_| "invalid provider timezone")?;
    let date_clock = ClockSpec {
        content: ClockContent::Date,
        ..clock.clone()
    };
    let selected: Vec<_> = h
        .time
        .iter()
        .enumerate()
        .filter(|(_, at)| **at > response.current.time)
        .take(settings.forecast_hours)
        .collect();
    let mut rain_summary = "Rain timing unavailable".to_owned();
    let mut next_rain = None;
    let mut complete = !selected.is_empty();
    for &(i, at) in &selected {
        match (rain_total(h, i), h.precipitation_probability[i]) {
            (Some(rain), Some(probability)) if complete => {
                if rain >= 0.1 && probability >= f64::from(settings.rain_threshold) {
                    let start = at.saturating_sub(3600);
                    next_rain = Some(start);
                    let today = date_clock
                        .format(response.current.time)
                        .map_err(|_| "invalid forecast time")?;
                    let day = date_clock
                        .format(start)
                        .map_err(|_| "invalid forecast time")?;
                    rain_summary = if start <= response.current.time {
                        format!("Rain likely this hour ({}%)", probability.round())
                    } else {
                        format!(
                            "Rain likely {}{}–{} ({}%)",
                            if day == today {
                                String::new()
                            } else {
                                format!("{day} ")
                            },
                            clock.format(start).map_err(|_| "invalid forecast time")?,
                            clock.format(*at).map_err(|_| "invalid forecast time")?,
                            probability.round()
                        )
                    };
                    break;
                }
            }
            _ => complete = false,
        }
    }
    if complete && next_rain.is_none() {
        rain_summary = "No likely rain in the forecast window".into();
    }
    let current = &response.current;
    let celsius = current.temperature_2m;
    let attribution = "open-meteo.com · CC BY 4.0 · rounded/converted";
    let row = BTreeMap::from([
        ("location".into(), json!(settings.location)),
        ("temperature".into(), json!(celsius)),
        (
            "display-temperature".into(),
            temperature(celsius, &settings),
        ),
        ("feels-like".into(), json!(current.apparent_temperature)),
        (
            "display-feels-like".into(),
            temperature(current.apparent_temperature, &settings),
        ),
        ("humidity".into(), json!(current.relative_humidity_2m)),
        ("wind-speed".into(), json!(current.wind_speed_10m)),
        ("weather-code".into(), json!(current.weather_code)),
        (
            "is-day".into(),
            json!(current.is_day.map(|value| value == 1)),
        ),
        (
            "icon-key".into(),
            json!(icon_key(current.weather_code, current.is_day)),
        ),
        ("summary".into(), json!(summary(current.weather_code))),
        (
            "symbol".into(),
            json!(symbol(current.weather_code, current.is_day)),
        ),
        ("rain-summary".into(), json!(rain_summary)),
        ("next-rain-at".into(), json!(next_rain)),
        ("timezone".into(), json!(response.timezone)),
        ("attribution".into(), json!(attribution)),
    ]);
    let current_dataset = dataset(
        "current-weather",
        vec![
            text("location"),
            number("temperature", "celsius"),
            text("display-temperature"),
            number("feels-like", "celsius"),
            text("display-feels-like"),
            number("humidity", "percent"),
            number("wind-speed", "km/h"),
            number("weather-code", "wmo"),
            Field {
                id: "is-day".into(),
                kind: FieldKind::Boolean,
                unit: None,
            },
            text("summary"),
            text("icon-key"),
            text("symbol"),
            text("rain-summary"),
            number("next-rain-at", "unix-seconds"),
            text("timezone"),
            text("attribution"),
        ],
        vec![row],
    );
    let hourly_rows = selected
        .into_iter()
        .map(|(i, at)| {
            Ok(BTreeMap::from([
                ("time".into(), json!(at)),
                (
                    "time-label".into(),
                    json!(clock.format(*at).map_err(|_| "invalid forecast time")?),
                ),
                ("temperature".into(), json!(h.temperature_2m[i])),
                (
                    "display-temperature".into(),
                    temperature(h.temperature_2m[i], &settings),
                ),
                ("rain".into(), json!(rain_total(h, i))),
                (
                    "precipitation-probability".into(),
                    json!(h.precipitation_probability[i]),
                ),
                ("weather-code".into(), json!(h.weather_code[i])),
                (
                    "icon-key".into(),
                    json!(icon_key(h.weather_code[i], h.is_day[i])),
                ),
                ("summary".into(), json!(summary(h.weather_code[i]))),
            ]))
        })
        .collect::<Result<Vec<_>>>()?;
    let hourly = dataset(
        "hourly",
        vec![
            number("time", "unix-seconds"),
            text("time-label"),
            number("temperature", "celsius"),
            text("display-temperature"),
            number("rain", "mm"),
            number("precipitation-probability", "percent"),
            number("weather-code", "wmo"),
            text("summary"),
            text("icon-key"),
        ],
        hourly_rows,
    );
    let daily_rows = d
        .time
        .iter()
        .enumerate()
        .take(settings.forecast_days)
        .map(|(i, at)| {
            Ok(BTreeMap::from([
                ("time".into(), json!(at)),
                (
                    "date".into(),
                    json!(
                        date_clock
                            .format(*at)
                            .map_err(|_| "invalid forecast time")?
                    ),
                ),
                ("min-temperature".into(), json!(d.temperature_2m_min[i])),
                ("max-temperature".into(), json!(d.temperature_2m_max[i])),
                (
                    "display-min".into(),
                    temperature(d.temperature_2m_min[i], &settings),
                ),
                (
                    "display-max".into(),
                    temperature(d.temperature_2m_max[i], &settings),
                ),
                ("precipitation".into(), json!(d.precipitation_sum[i])),
                (
                    "precipitation-probability".into(),
                    json!(d.precipitation_probability_max[i]),
                ),
                ("weather-code".into(), json!(d.weather_code[i])),
                (
                    "icon-key".into(),
                    json!(icon_key(d.weather_code[i], Some(1))),
                ),
                ("summary".into(), json!(summary(d.weather_code[i]))),
            ]))
        })
        .collect::<Result<Vec<_>>>()?;
    let daily = dataset(
        "daily",
        vec![
            number("time", "unix-seconds"),
            text("date"),
            number("min-temperature", "celsius"),
            number("max-temperature", "celsius"),
            text("display-min"),
            text("display-max"),
            number("precipitation", "mm"),
            number("precipitation-probability", "percent"),
            number("weather-code", "wmo"),
            text("summary"),
            text("icon-key"),
        ],
        daily_rows,
    );
    let publication = Publication {
        additional_observed_at: Default::default(),
        dataset: current_dataset,
        observed_at: Some(current.time),
        min_poll_secs: Some(600),
        additional_datasets: vec![hourly, daily],
    };
    publication.validate().map_err(|_| "invalid weather data")?;
    Ok(publication)
}
fn rain_total(hourly: &Hourly, index: usize) -> Option<f64> {
    hourly.rain[index]
        .zip(hourly.showers[index])
        .map(|(rain, showers)| rain + showers)
}
fn same_lengths(length: usize, others: &[usize]) -> bool {
    others.iter().all(|other| *other == length)
}
fn ascending(times: &[u64]) -> bool {
    times.windows(2).all(|pair| pair[0] < pair[1])
}
fn temperature(value: Option<f64>, settings: &Settings) -> Value {
    value.map_or(Value::Null, |value| {
        if settings.units == "imperial" {
            json!(format!("{:.1} °F", value * 1.8 + 32.0))
        } else {
            json!(format!("{value:.1} °C"))
        }
    })
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
fn icon_key(code: Option<u32>, day: Option<u8>) -> &'static str {
    match code {
        Some(0) => match day {
            Some(1) => "clear_day",
            Some(0) => "clear_night",
            _ => "unknown",
        },
        Some(1) => match day {
            Some(1) => "mainly_clear_day",
            Some(0) => "mainly_clear_night",
            _ => "unknown",
        },
        Some(2) => match day {
            Some(1) => "partly_cloudy_day",
            Some(0) => "partly_cloudy_night",
            _ => "unknown",
        },
        Some(3) => "overcast",
        Some(45) => "fog",
        Some(48) => "fog_rime",
        Some(51) => "drizzle_light",
        Some(53) => "drizzle_moderate",
        Some(55) => "drizzle_heavy",
        Some(56) => "freezing_drizzle_light",
        Some(57) => "freezing_drizzle_heavy",
        Some(61) => "rain_light",
        Some(63) => "rain_moderate",
        Some(65) => "rain_heavy",
        Some(66) => "freezing_rain_light",
        Some(67) => "freezing_rain_heavy",
        Some(71) => "snow_light",
        Some(73) => "snow_moderate",
        Some(75) => "snow_heavy",
        Some(77) => "snow_grains",
        Some(80) => "rain_showers_light",
        Some(81) => "rain_showers_moderate",
        Some(82) => "rain_showers_heavy",
        Some(85) => "snow_showers_light",
        Some(86) => "snow_showers_heavy",
        Some(95) => "thunderstorm",
        Some(96) => "thunderstorm_hail_light",
        Some(97) => "thunderstorm_heavy",
        Some(99) => "thunderstorm_hail_heavy",
        _ => "unknown",
    }
}

fn symbol(code: Option<u32>, day: Option<u8>) -> &'static str {
    match code {
        Some(0 | 1) => {
            if day == Some(0) {
                "☾"
            } else {
                "☀"
            }
        }
        Some(2 | 3) => "☁",
        Some(45 | 48) => "≋",
        Some(51..=67 | 80..=82) => "☂",
        Some(71..=77 | 85..=86) => "❄",
        Some(95..=99) => "⚡",
        _ => "?",
    }
}
fn summary(code: Option<u32>) -> &'static str {
    match code {
        Some(0) => "Clear sky",
        Some(1) => "Mainly clear",
        Some(2) => "Partly cloudy",
        Some(3) => "Overcast",
        Some(45 | 48) => "Fog",
        Some(51 | 53 | 55) => "Drizzle",
        Some(56 | 57) => "Freezing drizzle",
        Some(61) => "Light rain",
        Some(63) => "Rain",
        Some(65) => "Heavy rain",
        Some(66 | 67) => "Freezing rain",
        Some(71 | 73 | 75 | 77) => "Snow",
        Some(80..=82) => "Rain showers",
        Some(85 | 86) => "Snow showers",
        Some(95 | 97) => "Thunderstorm",
        Some(96 | 99) => "Thunderstorm with hail",
        _ => "Conditions unavailable",
    }
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    fn fetch() -> std::result::Result<(), ()> {
        let settings = paperphase_plugin_sdk::settings().map_err(|_| ())?;
        let source = paperphase_plugin_sdk::source_url().map_err(|_| ())?;
        let url = request_url(&source, &settings).map_err(|_| ())?;
        let bytes = paperphase_plugin_sdk::http_get(&url).map_err(|_| ())?;
        let publication = convert(&bytes, &settings).map_err(|_| ())?;
        paperphase_plugin_sdk::publish_with_metadata(&publication).map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_weather_icon_keys_exist_and_day_night_are_distinct() {
        let manifest: Value = serde_json::from_str(include_str!("../assets/icons.json")).unwrap();
        for code in [
            0, 1, 2, 3, 45, 48, 51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 71, 73, 75, 77, 80, 81, 82,
            85, 86, 95, 96, 97, 99, 999,
        ] {
            for day in [Some(0), Some(1), None] {
                assert!(manifest["icons"].get(icon_key(Some(code), day)).is_some());
            }
        }
        for code in [0, 1, 2] {
            assert_ne!(icon_key(Some(code), Some(0)), icon_key(Some(code), Some(1)));
        }
        assert_eq!(icon_key(None, None), "unknown");
        assert_eq!(icon_key(Some(3), Some(0)), "overcast");
        let publication =
            convert(include_bytes!("../fixtures/forecast.json"), &settings()).unwrap();
        for dataset in std::iter::once(&publication.dataset).chain(&publication.additional_datasets)
        {
            assert!(dataset.fields.iter().any(|field| field.id == "icon-key"));
            assert!(dataset.rows.iter().all(|row| {
                manifest["icons"]
                    .get(row["icon-key"].as_str().unwrap())
                    .is_some()
            }));
        }
    }
    fn settings() -> SettingsValues {
        serde_json::from_value(json!({"location":"Dublin","latitude":53.3498,"longitude":-6.2603,"units":"metric","forecast_hours":24,"forecast_days":3,"rain_threshold":50})).unwrap()
    }
    #[test]
    fn one_response_produces_current_hourly_daily_and_local_rain_timing() {
        let publication =
            convert(include_bytes!("../fixtures/forecast.json"), &settings()).unwrap();
        assert_eq!(publication.observed_at, Some(1700000000));
        assert_eq!(
            publication.dataset.rows[0]["display-temperature"],
            "12.5 °C"
        );
        assert_eq!(
            publication.dataset.rows[0]["rain-summary"],
            "Rain likely 23:00–00:00 (70%)"
        );
        assert_eq!(publication.additional_datasets[0].rows.len(), 8);
        assert_eq!(publication.additional_datasets[1].rows.len(), 3);
        assert_eq!(
            publication.additional_datasets[1].rows[0]["date"],
            "2023-11-14"
        );
        assert!(serde_json::to_vec(&publication).unwrap().len() < 65536);
        let mut imperial = settings();
        imperial.insert("units".into(), json!("imperial"));
        assert_eq!(
            convert(include_bytes!("../fixtures/forecast.json"), &imperial)
                .unwrap()
                .dataset
                .rows[0]["display-temperature"],
            "54.5 °F"
        );
    }
    #[test]
    fn nulls_unknown_codes_and_missing_rain_data_are_not_replaced_with_invented_values() {
        let mut response: Value =
            serde_json::from_slice(include_bytes!("../fixtures/forecast.json")).unwrap();
        response["current"]["temperature_2m"] = Value::Null;
        response["current"]["weather_code"] = json!(999);
        response["hourly"]["rain"][0] = Value::Null;
        let publication = convert(&serde_json::to_vec(&response).unwrap(), &settings()).unwrap();
        assert!(publication.dataset.rows[0]["display-temperature"].is_null());
        assert_eq!(
            publication.dataset.rows[0]["summary"],
            "Conditions unavailable"
        );
        assert_eq!(
            publication.dataset.rows[0]["rain-summary"],
            "Rain timing unavailable"
        );
    }
    #[test]
    fn bad_units_array_lengths_order_and_settings_fail_instead_of_replacing_good_content() {
        for (path, replacement) in [
            (["current_units", "temperature_2m"], json!("°F")),
            (["hourly", "rain"], json!([])),
            (["hourly", "time"], json!([2, 1])),
            (["timezone", ""], json!("Unknown/Zone")),
        ] {
            let mut response: Value =
                serde_json::from_slice(include_bytes!("../fixtures/forecast.json")).unwrap();
            if path[1].is_empty() {
                response[path[0]] = replacement;
            } else {
                response[path[0]][path[1]] = replacement;
            }
            assert!(convert(&serde_json::to_vec(&response).unwrap(), &settings()).is_err());
        }
        let mut invalid = settings();
        invalid.insert("latitude".into(), json!(91));
        assert!(request_url("https://api.open-meteo.com/v1/forecast", &invalid).is_err());
        let url = url::Url::parse(
            &request_url(
                "https://api.open-meteo.com/v1/forecast?old=value",
                &settings(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!url.query().unwrap().contains("old=value"));
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "latitude" && value == "53.3498")
        );
    }
}
