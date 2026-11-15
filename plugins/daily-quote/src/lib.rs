//! Daily online quotes with optional private, user-authored daily selections.
use chrono::{Datelike, TimeZone};
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
type Result<T> = std::result::Result<T, &'static str>;
#[derive(Deserialize)]
struct Personal {
    quote: String,
    #[serde(default)]
    author: String,
}
#[derive(Deserialize)]
struct Settings {
    mode: String,
    timezone: String,
    #[serde(default)]
    personal_quotes: Vec<Personal>,
}
fn settings(values: &SettingsValues) -> Result<Settings> {
    let s: Settings =
        serde_json::from_value(json!(values)).map_err(|_| "invalid quote settings")?;
    if !matches!(s.mode.as_str(), "online" | "personal" | "mixed")
        || s.timezone.parse::<chrono_tz::Tz>().is_err()
        || s.personal_quotes.len() > 16
        || s.personal_quotes
            .iter()
            .any(|q| q.quote.trim().is_empty() || q.quote.len() > 512 || q.author.len() > 96)
        || (s.mode == "personal" && s.personal_quotes.is_empty())
    {
        return Err("invalid quote settings");
    }
    Ok(s)
}
fn day(s: &Settings, now: u64) -> Result<chrono::DateTime<chrono_tz::Tz>> {
    let zone = s
        .timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| "invalid quote timezone")?;
    zone.timestamp_opt(i64::try_from(now).map_err(|_| "invalid quote time")?, 0)
        .single()
        .ok_or("invalid quote time")
}
pub fn uses_personal(values: &SettingsValues, now: u64) -> Result<bool> {
    let s = settings(values)?;
    let today = day(&s, now)?.date_naive().num_days_from_ce();
    Ok(s.mode == "personal"
        || s.mode == "mixed" && !s.personal_quotes.is_empty() && today.rem_euclid(2) == 0)
}
pub fn cached_online(previous: Option<&Dataset>, now: u64) -> Option<(Vec<u8>, u64)> {
    let previous = previous.filter(|d| d.id == "quote")?;
    let row = previous.rows.first()?;
    if row.get("source")?.as_str()? != "online" {
        return None;
    }
    let at = row.get("online-fetched-at")?.as_u64()?;
    if at > now || now - at >= 86400 {
        return None;
    }
    let q = row.get("quote")?.as_str()?;
    let a = row.get("author")?.as_str()?;
    if q.trim().is_empty()
        || q.len() > 1024
        || a.trim().is_empty()
        || a.len() > 128
        || a.eq_ignore_ascii_case("zenquotes.io")
    {
        return None;
    }
    // convert() still validates provider-shaped content and size.
    Some((serde_json::to_vec(&json!([{"q":q,"a":a}])).ok()?, at))
}

pub fn convert(bytes: Option<&[u8]>, values: &SettingsValues, now: u64) -> Result<Publication> {
    convert_with_cache(bytes, values, now, None)
}
pub fn convert_with_cache(
    bytes: Option<&[u8]>,
    values: &SettingsValues,
    now: u64,
    previous: Option<&Dataset>,
) -> Result<Publication> {
    let cached = cached_online(previous, now);
    let fetched_at = cached.as_ref().map_or(now, |(_, at)| *at);
    let bytes = cached.as_ref().map(|(bytes, _)| bytes.as_slice()).or(bytes);
    let s = settings(values)?;
    let local = day(&s, now)?;
    let personal = uses_personal(values, now)?;
    let (quote, author, source, attribution) = if personal {
        let index = (local
            .date_naive()
            .num_days_from_ce()
            .div_euclid(if s.mode == "mixed" { 2 } else { 1 }))
        .rem_euclid(s.personal_quotes.len() as i32) as usize;
        let q = &s.personal_quotes[index];
        (
            q.quote.trim().to_owned(),
            q.author.trim().to_owned(),
            "personal",
            "Your quote collection".to_owned(),
        )
    } else {
        let bytes = bytes.ok_or("online quote response missing")?;
        if bytes.len() > 65536 {
            return Err("quote response exceeds budget");
        }
        #[derive(Deserialize)]
        struct Online {
            q: String,
            a: String,
        }
        let rows: Vec<Online> =
            serde_json::from_slice(bytes).map_err(|_| "invalid quote response")?;
        if rows.len() != 1 {
            return Err("expected one daily quote");
        }
        let q = &rows[0];
        let quote = if cached.is_some() {
            q.q.trim().to_owned()
        } else {
            html_escape::decode_html_entities(&q.q).trim().to_owned()
        };
        let author = if cached.is_some() {
            q.a.trim().to_owned()
        } else {
            html_escape::decode_html_entities(&q.a).trim().to_owned()
        };
        if quote.is_empty()
            || quote.len() > 1024
            || author.is_empty()
            || author.len() > 128
            || author.eq_ignore_ascii_case("zenquotes.io")
        {
            return Err("invalid quote or provider error");
        }
        (
            quote,
            author,
            "online",
            "ZenQuotes API · https://zenquotes.io/".into(),
        )
    };
    let mut fields = ["quote", "author", "source", "selection-date", "attribution"]
        .map(|id| Field {
            id: id.into(),
            kind: FieldKind::Text,
            unit: None,
        })
        .to_vec();
    fields.push(Field {
        id: "online-fetched-at".into(),
        kind: FieldKind::Number,
        unit: Some("unix-seconds".into()),
    });
    let publication = Publication {
        dataset: Dataset {
            schema_version: 1,
            id: "quote".into(),
            fields,
            rows: vec![BTreeMap::from([
                ("quote".into(), json!(quote)),
                ("author".into(), json!(author)),
                ("source".into(), json!(source)),
                (
                    "selection-date".into(),
                    json!(local.format("%Y-%m-%d").to_string()),
                ),
                ("attribution".into(), json!(attribution)),
                (
                    "online-fetched-at".into(),
                    json!((!personal).then_some(fetched_at)),
                ),
            ])],
        },
        observed_at: (!personal).then_some(fetched_at),
        min_poll_secs: Some(60),
        additional_datasets: vec![],
        additional_observed_at: BTreeMap::new(),
    };
    publication
        .validate()
        .map_err(|_| "invalid quote publication")?;
    Ok(publication)
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    fn fetch() -> std::result::Result<(), ()> {
        let values = paperphase_plugin_sdk::settings().map_err(|_| ())?;
        let now = paperphase_plugin_sdk::unix_time().map_err(|_| ())?;
        let previous = paperphase_plugin_sdk::previous_dataset().map_err(|_| ())?;
        let bytes = if uses_personal(&values, now).map_err(|_| ())?
            || cached_online(previous.as_ref(), now).is_some()
        {
            None
        } else {
            let source = paperphase_plugin_sdk::source_url().map_err(|_| ())?;
            Some(paperphase_plugin_sdk::http_get(&source).map_err(|_| ())?)
        };
        let publication = convert_with_cache(bytes.as_deref(), &values, now, previous.as_ref())
            .map_err(|_| ())?;
        paperphase_plugin_sdk::publish_with_metadata(&publication).map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn values(mode: &str) -> SettingsValues {
        serde_json::from_value(json!({"mode":mode,"timezone":"Europe/Dublin","personal_quotes":[{"quote":"A small useful step.","author":"Example A"},{"quote":"Keep making room for ideas.","author":"Example B"}]})).unwrap()
    }
    #[test]
    fn provider_text_and_attribution_are_preserved_without_rendering_html() {
        let p = convert(
            Some(include_bytes!("../fixtures/today.json")),
            &values("online"),
            1700000000,
        )
        .unwrap();
        assert_eq!(p.dataset.rows[0]["source"], "online");
        assert_eq!(p.min_poll_secs, Some(60));
        assert!(p.dataset.rows[0]["quote"].as_str().unwrap().contains('&'));
        assert!(
            p.dataset.rows[0]["attribution"]
                .as_str()
                .unwrap()
                .contains("https://zenquotes.io/")
        );
    }
    #[test]
    fn personal_selection_is_stable_for_a_local_day_and_changes_daily() {
        let a = convert(None, &values("personal"), 1700000000).unwrap();
        let b = convert(None, &values("personal"), 1700000300).unwrap();
        let c = convert(None, &values("personal"), 1700086400).unwrap();
        assert_eq!(a.dataset.rows, b.dataset.rows);
        assert_ne!(a.dataset.rows[0]["quote"], c.dataset.rows[0]["quote"]);
        assert_eq!(a.min_poll_secs, Some(60));
        assert_ne!(
            uses_personal(&values("mixed"), 1700000000).unwrap(),
            uses_personal(&values("mixed"), 1700086400).unwrap()
        );
    }
    #[test]
    fn cached_online_quotes_keep_fetch_age_and_are_not_decoded_twice() {
        let p = convert(
            Some(br#"[{"q":"An &amp;amp; example.","a":"Fixture"}]"#),
            &values("online"),
            1700000000,
        )
        .unwrap();
        let cached =
            convert_with_cache(None, &values("online"), 1700000300, Some(&p.dataset)).unwrap();
        assert_eq!(cached.dataset.rows[0]["quote"], p.dataset.rows[0]["quote"]);
        assert_eq!(cached.observed_at, Some(1700000000));
        assert!(cached_online(Some(&p.dataset), 1700086400).is_none());
        let personal =
            convert_with_cache(None, &values("personal"), 1700000300, Some(&p.dataset)).unwrap();
        assert_eq!(personal.dataset.rows[0]["source"], "personal");
        assert!(personal.dataset.rows[0]["online-fetched-at"].is_null());
    }

    #[test]
    fn malformed_online_and_empty_personal_lists_fail() {
        for bytes in [
            b"[]".as_slice(),
            b"{}",
            br#"[{"q":"Too many requests","a":"zenquotes.io"}]"#,
        ] {
            assert!(convert(Some(bytes), &values("online"), 1700000000).is_err());
        }
        let mut v = values("personal");
        v.insert("personal_quotes".into(), json!([]));
        assert!(convert(None, &v, 1700000000).is_err());
    }
}
