//! Luas stop forecasts. One host GET serves both directions and service messages.
use chrono::{NaiveDateTime, TimeZone};
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use quick_xml::{Reader, events::Event};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, &'static str>;
const ATTRIBUTION: &str = "Luas / TII · CC BY 4.0 · formatted";
#[derive(Deserialize)]
struct Settings {
    stop: String,
    direction: String,
    max_arrivals: usize,
    #[serde(default)]
    destination: Option<String>,
}
#[derive(Deserialize)]
struct Stop {
    code: String,
    line: String,
}
impl Settings {
    fn parse(values: &SettingsValues) -> Result<(Self, Stop)> {
        let settings: Self =
            serde_json::from_value(json!(values)).map_err(|_| "invalid Luas settings")?;
        if !matches!(settings.direction.as_str(), "both" | "inbound" | "outbound")
            || !(1..=6).contains(&settings.max_arrivals)
            || settings
                .destination
                .as_ref()
                .is_some_and(|value| value.len() > 128)
        {
            return Err("invalid Luas settings");
        }
        let stops: Vec<Stop> = serde_json::from_str(include_str!("../stops.json"))
            .map_err(|_| "invalid stop catalog")?;
        let stop = stops
            .into_iter()
            .find(|stop| stop.code == settings.stop)
            .ok_or("unknown Luas stop")?;
        Ok((settings, stop))
    }
}
pub fn request_url(source: &str, values: &SettingsValues) -> Result<String> {
    let (settings, _) = Settings::parse(values)?;
    let mut url = url::Url::parse(source).map_err(|_| "invalid source URL")?;
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("action", "forecast")
        .append_pair("stop", &settings.stop.to_ascii_lowercase())
        .append_pair("encrypt", "false");
    Ok(url.into())
}
#[derive(Debug)]
struct Tram {
    direction: String,
    destination: String,
    minutes: Option<u16>,
}
struct Forecast {
    name: String,
    code: String,
    created: String,
    message: String,
    directions: BTreeSet<String>,
    trams: Vec<Tram>,
}

fn parse(bytes: &[u8]) -> Result<Forecast> {
    if bytes.len() > 65536 {
        return Err("forecast exceeds byte budget");
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "forecast must be UTF-8")?;
    let mut reader = Reader::from_str(text.trim_start_matches('\u{feff}'));
    let mut stack: Vec<String> = Vec::new();
    let mut root: Option<Forecast> = None;
    let mut closed = false;
    let mut direction = None;
    let mut message_seen = false;
    let mut trams_seen = 0;
    for _ in 0..4096 {
        let event = reader.read_event().map_err(|_| "invalid forecast XML")?;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                if closed || stack.len() >= 3 {
                    return Err("invalid forecast structure");
                }
                let name = tag.name().as_ref().to_owned();
                let mut attrs = BTreeMap::new();
                for attr in tag.attributes() {
                    let attr = attr.map_err(|_| "invalid or duplicate attribute")?;
                    let key = attr.key.as_ref().to_owned();
                    let value = attr
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| "invalid attribute value")?
                        .into_owned();
                    if key.len() > 64 || value.len() > 1024 || attrs.len() >= 32 {
                        return Err("attribute budget exceeded");
                    }
                    attrs.insert(key, value);
                }
                let required =
                    |key: &str| attrs.get(key).cloned().ok_or("missing forecast attribute");
                match (stack.last().map(String::as_str), name.as_str()) {
                    (None, "stopInfo") if root.is_none() => {
                        let name = clean(&required("stop")?);
                        let code = required("stopAbv")?;
                        let created = required("created")?;
                        if name.is_empty()
                            || name.len() > 128
                            || code.len() > 8
                            || created.len() > 32
                        {
                            return Err("invalid stop metadata");
                        }
                        root = Some(Forecast {
                            name,
                            code,
                            created,
                            message: String::new(),
                            directions: BTreeSet::new(),
                            trams: Vec::new(),
                        });
                    }
                    (Some("stopInfo"), "message") if !message_seen => {
                        message_seen = true;
                    }
                    (Some("stopInfo"), "direction") => {
                        let value = required("name")?;
                        if !matches!(value.as_str(), "Inbound" | "Outbound")
                            || !root
                                .as_mut()
                                .ok_or("missing root")?
                                .directions
                                .insert(value.clone())
                        {
                            return Err("invalid or duplicate direction");
                        }
                        direction = Some(value);
                    }
                    (Some("direction"), "tram") => {
                        trams_seen += 1;
                        if trams_seen > 128 {
                            return Err("tram budget exceeded");
                        }
                        let destination = clean(&required("destination")?);
                        let due = required("dueMins")?;
                        if destination.len() > 128 || due.len() > 32 {
                            return Err("tram text budget exceeded");
                        }
                        if due.trim().is_empty()
                            && (destination.is_empty()
                                || destination.eq_ignore_ascii_case("No trams forecast"))
                        {
                            // The provider emits this placeholder for empty terminal directions.
                        } else {
                            if destination.is_empty()
                                || destination.eq_ignore_ascii_case("No trams forecast")
                            {
                                return Err("invalid tram destination");
                            }
                            let minutes = if due.trim().eq_ignore_ascii_case("DUE") {
                                Some(0)
                            } else {
                                due.trim()
                                    .parse::<u16>()
                                    .ok()
                                    .filter(|minutes| *minutes <= 1440)
                            };
                            root.as_mut().ok_or("missing root")?.trams.push(Tram {
                                direction: direction.clone().ok_or("missing direction")?,
                                destination,
                                minutes,
                            });
                        }
                    }
                    _ => return Err("unexpected forecast element"),
                }
                if matches!(event, Event::Empty(_)) {
                    if name == "direction" {
                        direction = None;
                    }
                    if name == "stopInfo" {
                        closed = true;
                    }
                } else {
                    stack.push(name);
                }
            }
            Event::End(tag) => {
                let name = tag.name().as_ref().to_owned();
                if stack.pop().as_deref() != Some(name.as_str()) {
                    return Err("mismatched forecast element");
                }
                if name == "direction" {
                    direction = None;
                }
                if stack.is_empty() {
                    closed = true;
                }
            }
            Event::Text(text) => append(&mut root, &stack, &text.xml10_content())?,
            Event::CData(text) => {
                if stack.last().is_none_or(|name| name != "message") {
                    return Err("unexpected CDATA");
                }
                append(&mut root, &stack, &text.xml10_content())?;
            }
            Event::GeneralRef(reference) => {
                let value = if let Some(ch) = reference
                    .resolve_char_ref()
                    .map_err(|_| "invalid XML reference")?
                {
                    ch.to_string()
                } else {
                    match reference.xml10_content().as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => return Err("unknown XML entity"),
                    }
                    .to_owned()
                };
                if stack.last().is_none_or(|name| name != "message") {
                    return Err("unexpected XML reference");
                }
                append(&mut root, &stack, &value)?;
            }
            Event::Decl(declaration) => {
                if root.is_some()
                    || !declaration
                        .version()
                        .is_ok_and(|version| version.as_ref() == "1.0")
                    || declaration.encoding().is_some_and(|encoding| {
                        !encoding.is_ok_and(|value| {
                            value.eq_ignore_ascii_case("utf-8")
                                || value.eq_ignore_ascii_case("utf8")
                        })
                    })
                {
                    return Err("unsupported XML declaration");
                }
            }
            Event::DocType(_) | Event::PI(_) => {
                return Err("DTD and processing instructions are unsupported");
            }
            Event::Comment(_) => {}
            Event::Eof => {
                if !closed || !stack.is_empty() || !message_seen {
                    return Err("incomplete forecast");
                }
                let mut forecast = root.ok_or("missing stopInfo")?;
                if forecast.directions.is_empty() {
                    return Err("missing forecast directions");
                }
                forecast.message = clean(&forecast.message);
                return Ok(forecast);
            }
        }
    }
    Err("forecast event budget exceeded")
}
fn append(root: &mut Option<Forecast>, stack: &[String], value: &str) -> Result<()> {
    if stack.last().is_some_and(|name| name == "message") {
        let message = &mut root.as_mut().ok_or("missing root")?.message;
        if message.len() + value.len() > 1024 {
            return Err("service message budget exceeded");
        }
        message.push_str(value);
    } else if !value.trim().is_empty() {
        return Err("unexpected forecast text");
    }
    Ok(())
}
fn clean(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn observation(created: &str) -> Result<u64> {
    if created.len() != 19 || !created.is_ascii() {
        return Err("invalid forecast timestamp shape");
    }
    let local = NaiveDateTime::parse_from_str(created, "%Y-%m-%dT%H:%M:%S")
        .map_err(|_| "invalid forecast timestamp")?;
    // Feed timestamps have no offset. Choose the earlier instant on an autumn
    // clock overlap so uncertain data becomes stale early, never late.
    let at = chrono_tz::Europe::Dublin
        .from_local_datetime(&local)
        .earliest()
        .ok_or("nonexistent forecast timestamp")?;
    at.timestamp()
        .try_into()
        .map_err(|_| "invalid forecast epoch")
}
fn field(id: &str, kind: FieldKind) -> Field {
    Field {
        id: id.into(),
        kind,
        unit: None,
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
    let (settings, stop) = Settings::parse(values)?;
    let forecast = parse(bytes)?;
    if !forecast.code.eq_ignore_ascii_case(&stop.code) {
        return Err("forecast is for a different stop");
    }
    let at = observation(&forecast.created)?;
    let directions: Vec<&str> = match settings.direction.as_str() {
        "inbound" => vec!["Inbound"],
        "outbound" => vec!["Outbound"],
        _ => vec!["Inbound", "Outbound"],
    };
    let filter = settings
        .destination
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let mut selected = Vec::new();
    let mut boards = BTreeMap::new();
    for direction in &directions {
        let mut trams: Vec<_> = forecast
            .trams
            .iter()
            .filter(|tram| {
                tram.direction == *direction && tram.destination.to_lowercase().contains(&filter)
            })
            .collect();
        trams.sort_by_key(|tram| tram.minutes.unwrap_or(u16::MAX));
        trams.truncate(settings.max_arrivals);
        let text = if trams.is_empty() {
            if filter.is_empty() {
                "No trams forecast"
            } else {
                "No matching trams forecast"
            }
            .to_owned()
        } else {
            trams
                .iter()
                .map(|tram| format!("{} · {}", tram.destination, due_label(tram.minutes)))
                .collect::<Vec<_>>()
                .join("\n")
        };
        boards.insert(*direction, text);
        selected.extend(trams);
    }
    let label = if settings.direction == "both" {
        "Both directions"
    } else {
        directions[0]
    };
    let board = directions
        .iter()
        .map(|direction| format!("{direction}\n{}", boards[direction]))
        .collect::<Vec<_>>()
        .join("\n\n");
    let primary = dataset(
        "stop-board",
        vec![
            field("stop-name", FieldKind::Text),
            field("stop-code", FieldKind::Text),
            field("line", FieldKind::Text),
            field("direction-label", FieldKind::Text),
            field("service-message", FieldKind::Text),
            field("updated-label", FieldKind::Text),
            field("board", FieldKind::Text),
            field("inbound", FieldKind::Text),
            field("outbound", FieldKind::Text),
            field("arrival-count", FieldKind::Number),
            field("attribution", FieldKind::Text),
        ],
        vec![BTreeMap::from([
            ("stop-name".into(), json!(forecast.name)),
            ("stop-code".into(), json!(stop.code)),
            ("line".into(), json!(stop.line)),
            ("direction-label".into(), json!(label)),
            ("service-message".into(), json!(forecast.message)),
            (
                "updated-label".into(),
                json!(format!(
                    "Updated {} · Europe/Dublin",
                    &forecast.created[11..16]
                )),
            ),
            ("board".into(), json!(board)),
            (
                "inbound".into(),
                json!(
                    boards
                        .get("Inbound")
                        .map(String::as_str)
                        .unwrap_or("Direction not selected")
                ),
            ),
            (
                "outbound".into(),
                json!(
                    boards
                        .get("Outbound")
                        .map(String::as_str)
                        .unwrap_or("Direction not selected")
                ),
            ),
            ("arrival-count".into(), json!(selected.len())),
            ("attribution".into(), json!(ATTRIBUTION)),
        ])],
    );
    let arrivals = dataset(
        "arrivals",
        vec![
            field("direction", FieldKind::Text),
            field("destination", FieldKind::Text),
            field("due-minutes", FieldKind::Number),
            field("due-label", FieldKind::Text),
            field("status", FieldKind::Text),
            field("stop-code", FieldKind::Text),
        ],
        selected
            .into_iter()
            .map(|tram| {
                BTreeMap::from([
                    ("direction".into(), json!(tram.direction)),
                    ("destination".into(), json!(tram.destination)),
                    ("due-minutes".into(), json!(tram.minutes)),
                    ("due-label".into(), json!(due_label(tram.minutes))),
                    (
                        "status".into(),
                        json!(match tram.minutes {
                            Some(0) => "due",
                            Some(_) => "expected",
                            None => "unknown",
                        }),
                    ),
                    ("stop-code".into(), json!(stop.code)),
                ])
            })
            .collect(),
    );
    let publication = Publication {
        dataset: primary,
        observed_at: Some(at),
        min_poll_secs: Some(60),
        additional_datasets: vec![arrivals],
        additional_observed_at: BTreeMap::from([("arrivals".into(), at)]),
    };
    publication
        .validate()
        .map_err(|_| "invalid Luas publication")?;
    Ok(publication)
}
fn due_label(minutes: Option<u16>) -> String {
    match minutes {
        Some(0) => "Due".into(),
        Some(n) => format!("{n} min"),
        None => "Time unavailable".into(),
    }
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
        paperphase_plugin_sdk::publish_with_metadata(&convert(&bytes, &settings).map_err(|_| ())?)
            .map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> SettingsValues {
        serde_json::from_value(
            json!({"stop":"RAN","direction":"both","max_arrivals":3,"destination":""}),
        )
        .unwrap()
    }
    #[test]
    fn both_directions_sort_due_and_numeric_arrivals_and_publish_observation_times() {
        let p = convert(include_bytes!("../fixtures/forecast.xml"), &settings()).unwrap();
        assert_eq!(p.observed_at, Some(1700000000));
        assert_eq!(p.additional_observed_at["arrivals"], 1700000000);
        assert_eq!(p.min_poll_secs, Some(60));
        assert_eq!(
            p.dataset.rows[0]["inbound"],
            "Parnell · Due\nBroombridge · 7 min\nBroombridge · 12 min"
        );
        assert_eq!(
            p.dataset.rows[0]["outbound"],
            "Sandyford · Due\nBrides Glen · 6 min\nSandyford · 14 min"
        );
        assert_eq!(p.dataset.rows[0]["arrival-count"], 6);
        assert_eq!(p.additional_datasets[0].rows[0]["due-minutes"], 0);
        assert_eq!(p.additional_datasets[0].rows[0]["status"], "due");
        assert!(serde_json::to_vec(&p).unwrap().len() < 65536);
    }
    #[test]
    fn filters_unknown_times_and_successful_empty_results_have_distinct_meanings() {
        let mut s = settings();
        s.insert("direction".into(), json!("inbound"));
        s.insert("destination".into(), json!("pArNeLl"));
        let p = convert(include_bytes!("../fixtures/forecast.xml"), &s).unwrap();
        assert_eq!(p.additional_datasets[0].rows.len(), 2);
        assert!(p.additional_datasets[0].rows[1]["due-minutes"].is_null());
        assert_eq!(p.additional_datasets[0].rows[1]["status"], "unknown");
        assert_eq!(
            p.additional_datasets[0].rows[1]["due-label"],
            "Time unavailable"
        );
        assert_eq!(p.dataset.rows[0]["outbound"], "Direction not selected");
        s.insert("destination".into(), json!("Not a destination"));
        let filtered = convert(include_bytes!("../fixtures/forecast.xml"), &s).unwrap();
        assert_eq!(
            filtered.dataset.rows[0]["inbound"],
            "No matching trams forecast"
        );
        let empty = convert(include_bytes!("../fixtures/empty.xml"), &settings()).unwrap();
        assert_eq!(empty.dataset.rows[0]["arrival-count"], 0);
        assert_eq!(
            empty.dataset.rows[0]["service-message"],
            "No services operating"
        );
        assert!(empty.additional_datasets[0].rows.is_empty());
        assert_eq!(empty.dataset.rows[0]["inbound"], "No trams forecast");
    }
    #[test]
    fn timestamps_use_dublin_dst_and_ambiguous_hours_become_stale_conservatively() {
        assert_eq!(observation("2024-07-01T12:00:00").unwrap(), 1719831600);
        assert_eq!(observation("2024-10-27T01:30:00").unwrap(), 1729989000);
        for value in [
            "2024-03-31T01:30:00",
            "2024-02-30T12:00:00",
            "2024-1-1T1:1:1",
            "bad",
            "2024-11-10T23:00:00Z",
        ] {
            assert!(observation(value).is_err(), "{value}");
        }
    }
    #[test]
    fn xml_entities_cdata_single_platform_and_request_identity_are_handled() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><stopInfo stop="Ranelagh" stopAbv="ran" created="2023-11-14T22:13:20"><message>Wait &amp; <![CDATA[check <updates>]]></message><direction name="Outbound"><tram dueMins="5" destination="A &amp; B" /></direction></stopInfo>"#;
        let p = convert(xml.as_bytes(), &settings()).unwrap();
        assert_eq!(
            p.dataset.rows[0]["service-message"],
            "Wait & check <updates>"
        );
        assert_eq!(p.dataset.rows[0]["inbound"], "No trams forecast");
        assert_eq!(p.additional_datasets[0].rows[0]["destination"], "A & B");
        let url = url::Url::parse(
            &request_url("https://example.com/get.ashx?bad=1", &settings()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![
                ("action".into(), "forecast".into()),
                ("stop".into(), "ran".into()),
                ("encrypt".into(), "false".into())
            ]
        );
        assert!(
            convert(
                xml.replace("stopAbv=\"ran\"", "stopAbv=\"CHA\"").as_bytes(),
                &settings()
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_xml_and_resource_exhaustion_fail_before_publishing() {
        let original = include_str!("../fixtures/forecast.xml");
        for xml in [
            "<html>Error</html>".to_owned(),
            original.replace("</stopInfo>", ""),
            original.replace("<message>", "<message>&unknown;"),
            original.replace("created=", "created=\"duplicate\" created="),
            format!("<!DOCTYPE stopInfo [<!ENTITY x SYSTEM 'file:///etc/passwd'>]>{original}"),
            format!("{original}{original}"),
            original.replace("</stopInfo>", "<message>Duplicate</message></stopInfo>"),
            original.replace("</stopInfo>", "<direction name=\"Inbound\"/></stopInfo>"),
            original.replace("Green Line services operating normally", &"X".repeat(1025)),
            original.replace(
                "<tram dueMins=\"12\" destination=\"Broombridge\"/>",
                &"<tram dueMins=\"2\" destination=\"Parnell\"/>".repeat(129),
            ),
        ] {
            assert!(convert(xml.as_bytes(), &settings()).is_err());
        }
        assert!(convert(&vec![b' '; 65537], &settings()).is_err());
        assert!(convert(b"\xff", &settings()).is_err());
        for (key, value) in [
            ("stop", json!("NOTREAL")),
            ("direction", json!("north")),
            ("max_arrivals", json!(7)),
            ("destination", json!("X".repeat(129))),
        ] {
            let mut s = settings();
            s.insert(key.into(), value);
            assert!(convert(original.as_bytes(), &s).is_err());
        }
    }
    #[test]
    fn stop_selector_and_guest_catalog_agree_for_all_stops_and_defaults() {
        let stops: Vec<Stop> = serde_json::from_str(include_str!("../stops.json")).unwrap();
        let catalog: Value =
            serde_json::from_str(include_str!("../../../examples/simulator/connectors.json"))
                .unwrap();
        let definition = catalog["connectors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "transport.luas")
            .unwrap();
        let options = definition["settings"][0]["input"]["options"]
            .as_array()
            .unwrap();
        assert_eq!(stops.len(), 67);
        assert_eq!(options.len(), stops.len());
        for stop in stops {
            assert!(options.iter().any(|option| option["value"] == stop.code));
        }
        let mut defaults = settings();
        defaults.insert("stop".into(), json!("CHA"));
        assert!(Settings::parse(&defaults).is_ok());
    }
}
