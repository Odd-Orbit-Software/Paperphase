//! Read-only Calendar events. OAuth credentials remain in the host.
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, &'static str>;

#[derive(Deserialize)]
struct Settings {
    calendar_id: String,
    source_label: String,
    timezone: String,
    days_ahead: u32,
    max_events: usize,
    show_titles: bool,
    show_locations: bool,
    show_descriptions: bool,
    include_all_day: bool,
}

fn settings(values: &SettingsValues) -> Result<(Settings, Tz)> {
    let settings: Settings =
        serde_json::from_value(json!(values)).map_err(|_| "invalid Google Calendar settings")?;
    let tz = settings
        .timezone
        .parse()
        .map_err(|_| "invalid calendar timezone")?;
    if settings.calendar_id.is_empty()
        || settings.calendar_id.len() > 1024
        || settings.calendar_id.chars().any(char::is_control)
        || settings.source_label.trim().is_empty()
        || settings.source_label.len() > 128
        || !(1..=14).contains(&settings.days_ahead)
        || !(1..=12).contains(&settings.max_events)
    {
        return Err("invalid Google Calendar settings");
    }
    Ok((settings, tz))
}

fn text(value: Option<&Value>, max: usize) -> String {
    bounded(value.and_then(Value::as_str).unwrap_or(""), max)
}

fn bounded(value: &str, max: usize) -> String {
    let mut end = value.len().min(max);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}

fn timestamp(value: &Value, timezone: Tz) -> Result<(i64, bool)> {
    if let Some(date) = value.get("date").and_then(Value::as_str) {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| "invalid all-day event date")?;
        let at = timezone
            .from_local_datetime(
                &date
                    .and_hms_opt(0, 0, 0)
                    .ok_or("invalid all-day event date")?,
            )
            .single()
            .ok_or("ambiguous all-day event boundary")?;
        return Ok((at.timestamp(), true));
    }
    let value = value
        .get("dateTime")
        .and_then(Value::as_str)
        .ok_or("calendar event has no start or end")?;
    let at = DateTime::parse_from_rfc3339(value).map_err(|_| "invalid calendar event timestamp")?;
    Ok((at.timestamp(), false))
}

fn plain_description(value: &Value) -> String {
    let raw = text(Some(value), 4096);
    let mut inside = false;
    let stripped: String = raw
        .chars()
        .filter(|ch| match ch {
            '<' => {
                inside = true;
                false
            }
            '>' => {
                inside = false;
                false
            }
            _ => !inside,
        })
        .collect();
    bounded(&html_escape::decode_html_entities(&stripped), 1024)
}

pub fn convert(bytes: &[u8], values: &SettingsValues, now: u64) -> Result<Publication> {
    if bytes.len() > 256 * 1024 || now > i64::MAX as u64 {
        return Err("calendar response exceeds budget");
    }
    let (settings, timezone) = settings(values)?;
    let document: Value = serde_json::from_slice(bytes).map_err(|_| "invalid calendar JSON")?;
    let events = document
        .get("items")
        .and_then(Value::as_array)
        .ok_or("calendar response has no event collection")?;
    if events.len() > 250 {
        return Err("calendar event count exceeds budget");
    }
    let end = (now as i64)
        .checked_add(i64::from(settings.days_ahead) * 86400)
        .ok_or("calendar window is out of range")?;
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for event in events {
        if event.get("status").and_then(Value::as_str) == Some("cancelled") {
            continue;
        }
        let id = text(event.get("id"), 256);
        if id.is_empty() {
            return Err("calendar event has no identifier");
        }
        let (starts, all_day) = timestamp(&event["start"], timezone)?;
        let (ends, end_all_day) = timestamp(&event["end"], timezone)?;
        if starts < 0 || ends <= starts || all_day != end_all_day {
            return Err("invalid calendar event interval");
        }
        if ends <= now as i64 || starts >= end || (all_day && !settings.include_all_day) {
            continue;
        }
        if !seen.insert((id.clone(), starts)) {
            continue;
        }
        let free = event.get("transparency").and_then(Value::as_str) == Some("transparent");
        let title = if settings.show_titles {
            let title = text(event.get("summary"), 256);
            if title.trim().is_empty() {
                "Untitled event".into()
            } else {
                title
            }
        } else if free {
            "Available".into()
        } else {
            "Busy".into()
        };
        let local = Utc
            .timestamp_opt(starts, 0)
            .single()
            .ok_or("calendar timestamp out of range")?
            .with_timezone(&timezone);
        let when = if all_day {
            format!("{} · All day", local.format("%a %d %b"))
        } else {
            local.format("%a %d %b · %H:%M").to_string()
        };
        let mut row = BTreeMap::from([
            ("event-id".into(), json!(id)),
            ("ical-uid".into(), json!(text(event.get("iCalUID"), 256))),
            ("calendar-id".into(), json!(settings.calendar_id)),
            ("source".into(), json!(settings.source_label)),
            ("title".into(), json!(title)),
            ("when".into(), json!(when)),
            ("start-at".into(), json!(starts)),
            ("end-at".into(), json!(ends)),
            ("all-day".into(), json!(all_day)),
            (
                "availability".into(),
                json!(if free { "free" } else { "busy" }),
            ),
            ("status".into(), json!(text(event.get("status"), 32))),
        ]);
        row.insert(
            "location".into(),
            json!(if settings.show_locations {
                text(event.get("location"), 512)
            } else {
                String::new()
            }),
        );
        row.insert(
            "description".into(),
            json!(if settings.show_descriptions {
                plain_description(&event["description"])
            } else {
                String::new()
            }),
        );
        rows.push(row);
    }
    rows.sort_by(|a, b| {
        a["start-at"]
            .as_i64()
            .cmp(&b["start-at"].as_i64())
            .then(a["event-id"].as_str().cmp(&b["event-id"].as_str()))
    });
    if document
        .get("nextPageToken")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
        && rows.len() < settings.max_events
    {
        return Err("calendar result incomplete; narrow the window or include all-day events");
    }
    rows.truncate(settings.max_events);
    let fields = [
        "event-id",
        "ical-uid",
        "calendar-id",
        "source",
        "title",
        "when",
        "location",
        "description",
        "availability",
        "status",
    ]
    .map(|id| Field {
        id: id.into(),
        kind: FieldKind::Text,
        unit: None,
    })
    .into_iter()
    .chain(["start-at", "end-at"].map(|id| Field {
        id: id.into(),
        kind: FieldKind::Number,
        unit: Some("unix-seconds".into()),
    }))
    .chain([Field {
        id: "all-day".into(),
        kind: FieldKind::Boolean,
        unit: None,
    }])
    .collect();
    let summary = if rows.is_empty() {
        "No upcoming events".into()
    } else {
        rows.iter()
            .map(|row| {
                format!(
                    "{}  {}",
                    row["when"].as_str().unwrap(),
                    row["title"]
                        .as_str()
                        .unwrap()
                        .chars()
                        .take(160)
                        .collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let summary = bounded(&summary, 4096);
    let result = Publication {
        dataset: Dataset {
            schema_version: 1,
            id: "events".into(),
            fields,
            rows,
        },
        observed_at: Some(now),
        min_poll_secs: Some(60),
        additional_datasets: vec![Dataset {
            schema_version: 1,
            id: "agenda".into(),
            fields: ["title", "summary"]
                .map(|id| Field {
                    id: id.into(),
                    kind: FieldKind::Text,
                    unit: None,
                })
                .into(),
            rows: vec![BTreeMap::from([
                ("title".into(), json!(settings.source_label)),
                ("summary".into(), json!(summary)),
            ])],
        }],
        additional_observed_at: BTreeMap::new(),
    };
    result
        .validate()
        .map_err(|_| "calendar publication exceeds contract")?;
    Ok(result)
}

pub fn source_url(source: &str, values: &SettingsValues, now: u64) -> Result<String> {
    let (settings, _) = settings(values)?;
    let mut url = url::Url::parse(source).map_err(|_| "invalid calendar source")?;
    url.path_segments_mut()
        .map_err(|_| "invalid calendar source")?
        .pop_if_empty()
        .extend(["calendars", &settings.calendar_id, "events"]);
    let start =
        DateTime::from_timestamp(i64::try_from(now).map_err(|_| "invalid calendar time")?, 0)
            .ok_or("invalid calendar time")?;
    let end = start
        .checked_add_signed(chrono::Duration::days(i64::from(settings.days_ahead)))
        .ok_or("calendar time out of range")?;
    let mut fields = "nextPageToken,items(id,iCalUID,status,transparency,start,end".to_string();
    if settings.show_titles {
        fields.push_str(",summary");
    }
    if settings.show_locations {
        fields.push_str(",location");
    }
    if settings.show_descriptions {
        fields.push_str(",description");
    }
    fields.push(')');
    url.query_pairs_mut()
        .append_pair("singleEvents", "true")
        .append_pair("orderBy", "startTime")
        .append_pair("showDeleted", "false")
        .append_pair("maxResults", "250")
        .append_pair("timeMin", &start.to_rfc3339())
        .append_pair("timeMax", &end.to_rfc3339())
        .append_pair("timeZone", &settings.timezone)
        .append_pair("fields", &fields);
    Ok(url.into())
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    fn fetch() -> std::result::Result<(), ()> {
        let now = paperphase_plugin_sdk::unix_time().map_err(|_| ())?;
        let settings = paperphase_plugin_sdk::settings().map_err(|_| ())?;
        let source = paperphase_plugin_sdk::source_url().map_err(|_| ())?;
        let url = source_url(&source, &settings, now).map_err(|_| ())?;
        let bytes = paperphase_plugin_sdk::http_get_bounded(&url, 256 * 1024).map_err(|_| ())?;
        let publication = convert(&bytes, &settings, now).map_err(|_| ())?;
        paperphase_plugin_sdk::publish_with_metadata(&publication).map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn values() -> SettingsValues {
        serde_json::from_value(json!({"calendar_id":"primary","source_label":"Personal","timezone":"Europe/Dublin","days_ahead":7,"max_events":8,"show_titles":true,"show_locations":false,"show_descriptions":false,"include_all_day":true})).unwrap()
    }
    #[test]
    fn filters_sorts_and_keeps_all_day_exclusive_end_dates() {
        let p = convert(
            include_bytes!("../fixtures/events.json"),
            &values(),
            1794700800,
        )
        .unwrap();
        assert_eq!(p.dataset.rows.len(), 3);
        assert!(p.dataset.rows[0]["all-day"].as_bool().unwrap());
        assert_eq!(
            p.dataset.rows[0]["end-at"].as_i64().unwrap()
                - p.dataset.rows[0]["start-at"].as_i64().unwrap(),
            86400
        );
        assert_eq!(p.dataset.rows[1]["title"], "Morning meeting");
        assert_eq!(p.dataset.rows[2]["title"], "Evening appointment");
        assert!(
            p.dataset
                .rows
                .iter()
                .all(|r| r["description"] == "" && r["location"] == "")
        );
    }
    #[test]
    fn privacy_is_applied_to_both_datasets_and_requested_fields() {
        let mut s = values();
        s.insert("show_titles".into(), json!(false));
        let p = convert(include_bytes!("../fixtures/events.json"), &s, 1794700800).unwrap();
        let encoded = serde_json::to_string(&p).unwrap();
        for private in ["Morning meeting", "Private location", "Private description"] {
            assert!(!encoded.contains(private));
        }
        assert!(encoded.contains("Busy"));
        let url = url::Url::parse(
            &source_url("https://www.googleapis.com/calendar/v3", &s, 1794700800).unwrap(),
        )
        .unwrap();
        let fields = url
            .query_pairs()
            .find(|(k, _)| k == "fields")
            .unwrap()
            .1
            .into_owned();
        assert!(
            !fields.contains("summary")
                && !fields.contains("description")
                && !fields.contains("location")
        );
        assert!(url.as_str().contains("singleEvents=true"));
    }
    #[test]
    fn empty_and_incomplete_results_are_distinct() {
        assert!(
            convert(br#"{"items":[]}"#, &values(), 1794700800)
                .unwrap()
                .dataset
                .rows
                .is_empty()
        );
        assert!(
            convert(
                br#"{"items":[],"nextPageToken":"more"}"#,
                &values(),
                1794700800
            )
            .is_err()
        );
        assert!(convert(br#"{"error":{"message":"bad"}}"#, &values(), 1794700800).is_err());
    }
    #[test]
    fn calendar_identifiers_are_encoded_as_one_path_segment() {
        let mut s = values();
        s.insert("calendar_id".into(), json!("shared/a+b@example.org"));
        let url = source_url("https://www.googleapis.com/calendar/v3", &s, 1794700800).unwrap();
        assert!(url.contains("shared%2Fa+b@example.org/events"));
        s.insert("timezone".into(), json!("invalid"));
        assert!(source_url("https://www.googleapis.com/calendar/v3", &s, 1794700800).is_err());
    }
}

#[cfg(test)]
mod calendar_edge_tests {
    use super::*;
    #[test]
    fn all_day_intervals_follow_dst_and_unicode_is_bounded_by_bytes() {
        let values:SettingsValues = serde_json::from_value(json!({"calendar_id":"primary","source_label":"Personal","timezone":"Europe/Dublin","days_ahead":7,"max_events":12,"show_titles":true,"show_locations":true,"show_descriptions":true,"include_all_day":true})).unwrap();
        for (start, end, hours) in [
            ("2026-03-29", "2026-03-30", 23),
            ("2026-10-25", "2026-10-26", 25),
        ] {
            let now = DateTime::parse_from_rfc3339(&format!("{start}T00:00:00Z"))
                .unwrap()
                .timestamp() as u64;
            let events:Vec<_> = (0..12).map(|id| json!({"id":format!("event-{id}"),"summary":"🌍".repeat(256),"location":"🌍".repeat(512),"description":format!("<b>{}</b>","🌍".repeat(1024)),"start":{"date":start},"end":{"date":end}})).collect();
            let publication = convert(
                &serde_json::to_vec(&json!({"items":events})).unwrap(),
                &values,
                now,
            )
            .unwrap();
            let row = &publication.dataset.rows[0];
            assert_eq!(
                row["end-at"].as_i64().unwrap() - row["start-at"].as_i64().unwrap(),
                hours * 3600
            );
            assert!(row["description"].as_str().unwrap().len() <= 1024);
            assert!(
                publication.additional_datasets[0].rows[0]["summary"]
                    .as_str()
                    .unwrap()
                    .len()
                    <= 4096
            );
            assert!(serde_json::to_vec(&publication).unwrap().len() < 65536);
        }
    }
}
