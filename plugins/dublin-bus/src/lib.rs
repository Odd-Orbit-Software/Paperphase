//! One bounded national feed supplies all selected Dublin Bus stop boards.
mod proto;
mod timetable;
use paperphase_plugin_sdk::clock::{ClockContent, ClockSpec, DateFormat, HourCycle};
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, &'static str>;
#[derive(Deserialize)]
struct Settings {
    stop_numbers: String,
    routes: Option<String>,
    max_arrivals: usize,
}
impl Settings {
    fn parse(values: &SettingsValues) -> Result<(Self, Vec<String>, BTreeSet<String>)> {
        let settings: Self =
            serde_json::from_value(json!(values)).map_err(|_| "invalid bus settings")?;
        if settings.stop_numbers.len() > 512
            || !(1..=6).contains(&settings.max_arrivals)
            || settings.routes.as_ref().is_some_and(|r| r.len() > 256)
        {
            return Err("invalid bus settings");
        }
        let mut stops = Vec::new();
        for token in settings
            .stop_numbers
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|s| !s.is_empty())
        {
            if !token.bytes().all(|b| b.is_ascii_digit()) || token.len() > 6 {
                return Err("stop numbers must contain digits");
            }
            let number = token.parse::<u32>().map_err(|_| "invalid stop number")?;
            if number == 0 {
                return Err("invalid stop number");
            }
            let number = number.to_string();
            if !stops.contains(&number) {
                stops.push(number);
            }
            if stops.len() > 8 {
                return Err("at most eight stops are supported");
            }
        }
        if stops.is_empty() {
            return Err("at least one stop is required");
        }
        let mut routes = BTreeSet::new();
        for token in settings
            .routes
            .as_deref()
            .unwrap_or("")
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|s| !s.is_empty())
        {
            if token.len() > 12 || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Err("invalid route filter");
            }
            routes.insert(token.to_ascii_uppercase());
        }
        if routes.len() > 16 {
            return Err("too many route filters");
        }
        Ok((settings, stops, routes))
    }
}
struct Stop {
    number: String,
    id: String,
    name: String,
    rows: Vec<Arrival>,
    observed: u64,
    missing_details: bool,
}
struct Arrival {
    route: String,
    destination: String,
    at: Option<u64>,
    label: String,
    status: &'static str,
    trip: String,
    observed: u64,
    details: bool,
}
// GTFS service times count from local noon minus twelve hours, including DST days.
fn scheduled_time(date: &str, time: &str, delay: i32) -> Option<u64> {
    use chrono::TimeZone;
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let day = chrono::NaiveDate::parse_from_str(date, "%Y%m%d").ok()?;
    let noon = chrono_tz::Europe::Dublin
        .from_local_datetime(&day.and_hms_opt(12, 0, 0)?)
        .single()?
        .timestamp();
    let parts = time.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    let hours = parts[0].parse::<i64>().ok()?;
    let minutes = parts[1].parse::<i64>().ok()?;
    let seconds = parts[2].parse::<i64>().ok()?;
    if !(0..=71).contains(&hours) || !(0..60).contains(&minutes) || !(0..60).contains(&seconds) {
        return None;
    }
    u64::try_from(noon - 43200 + hours * 3600 + minutes * 60 + seconds + i64::from(delay)).ok()
}
fn lookup<F>(query: &mut F, table: &str, key: &str) -> Result<Option<Vec<String>>>
where
    F: FnMut(&str, &str) -> Result<Option<Vec<String>>>,
{
    query(table, key)
}
fn stops<F>(numbers: &[String], query: &mut F) -> Result<Vec<Stop>>
where
    F: FnMut(&str, &str) -> Result<Option<Vec<String>>>,
{
    numbers
        .iter()
        .map(|number| {
            let row = lookup(query, "stops", number)?
                .filter(|row| row.len() >= 2 && !row[0].is_empty() && !row[1].is_empty())
                .ok_or("unknown stop or reference data unavailable")?;
            Ok(Stop {
                number: number.clone(),
                id: row[0].clone(),
                name: row[1].clone(),
                rows: Vec::new(),
                observed: u64::MAX,
                missing_details: false,
            })
        })
        .collect()
}
pub fn request_url(source: &str) -> Result<String> {
    let mut url = url::Url::parse(source).map_err(|_| "invalid feed URL")?;
    url.set_query(None);
    Ok(url.into())
}
fn text(id: &str) -> Field {
    Field {
        id: id.into(),
        kind: FieldKind::Text,
        unit: None,
    }
}
fn number(id: &str) -> Field {
    Field {
        id: id.into(),
        kind: FieldKind::Number,
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
pub fn convert<F>(bytes: &[u8], values: &SettingsValues, mut source_query: F) -> Result<Publication>
where
    F: FnMut(&str, &str) -> Result<Option<Vec<String>>>,
{
    let mut cache = BTreeMap::<(String, String), Option<Vec<String>>>::new();
    let mut query = |table: &str, key: &str| {
        if table == "times" {
            return source_query(table, key);
        }
        let cache_key = (table.to_owned(), key.to_owned());
        if let Some(value) = cache.get(&cache_key) {
            return Ok(value.clone());
        }
        let value = source_query(table, key)?;
        if cache.len() >= 4096 {
            return Err("timetable lookup cache budget exceeded");
        }
        cache.insert(cache_key, value.clone());
        Ok(value)
    };
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("feed exceeds input budget");
    }
    let (settings, numbers, routes) = Settings::parse(values)?;
    let mut stops = stops(&numbers, &mut query)?;
    let header = proto::header(bytes)?;
    let version_matches = if let Some(version) = header.version {
        lookup(&mut query, "feed", version)?.is_some()
    } else {
        true
    };
    let timetables = if version_matches {
        timetable::load(&stops, &mut query)?
    } else {
        BTreeMap::new()
    };
    let mut covered = BTreeSet::<(String, String, u64)>::new();
    let clock = ClockSpec {
        timezone: "Europe/Dublin".into(),
        content: ClockContent::Time,
        hour_cycle: HourCycle::TwentyFour,
        date_format: DateFormat::Iso,
    };
    let mut root = proto::Reader::new(bytes);
    let mut stop_count = 0;
    while let Some((field, value)) = root.next()? {
        if field != 2 {
            continue;
        }
        let mut entity = proto::Reader::new(value.bytes()?);
        let mut update = None;
        let mut deleted = false;
        let mut entity_id = "";
        while let Some((field, value)) = entity.next()? {
            match field {
                1 => entity_id = value.text()?,
                2 => deleted = value.number()? != 0,
                3 => update = Some(value.bytes()?),
                _ => {}
            }
        }
        if deleted {
            continue;
        }
        let Some(update) = update else {
            continue;
        };
        let trip = proto::trip(update)?;
        let trip_key = if trip.id.is_empty() {
            entity_id
        } else {
            trip.id
        };
        let observed = trip
            .observed
            .map_or(header.timestamp, |at| at.min(header.timestamp));
        let schedule = timetables.get(trip.id).map(Vec::as_slice).unwrap_or(&[]);
        let mut reader = proto::Reader::new(update);
        let mut calls = Vec::new();
        while let Some((field, value)) = reader.next()? {
            if field != 2 {
                continue;
            }
            stop_count += 1;
            if stop_count > 500_000 || calls.len() >= 512 {
                return Err("feed stop-update budget exceeded");
            }
            let mut call = proto::stop_time(value.bytes()?)?;
            if call.stop.is_empty()
                && let Some(sequence) = call.sequence
                && let Some(row) = schedule.iter().find(|row| row.sequence == sequence)
            {
                call.stop = &row.stop;
            }
            calls.push(call);
        }
        // Sparse updates carry their delay forward until the next update or NO_DATA.
        // SKIPPED calls do not interrupt delay propagation. Never back-propagate.
        for row in schedule {
            let sequence = row.sequence;
            if !stops.iter().any(|s| s.id == row.stop)
                || calls.iter().any(|c| {
                    c.sequence == Some(sequence) || (c.sequence.is_none() && c.stop == row.stop)
                })
            {
                continue;
            }
            let previous = calls
                .iter()
                .filter(|c| c.sequence.is_some_and(|n| n < sequence) && c.relationship != 1)
                .max_by_key(|c| c.sequence);
            let delay = previous
                .filter(|c| c.relationship != 2)
                .and_then(|c| c.delay);
            calls.push(proto::StopTime {
                stop: &row.stop,
                time: None,
                sequence: Some(sequence),
                delay,
                departure: true,
                relationship: 0,
            });
        }
        let mut details: Option<(String, String, bool)> = None;
        for call in calls {
            // A sequence pointing to a reassigned stop also suppresses fallback
            // at the original stop; never re-advertise a skipped/reassigned call.
            for row in schedule.iter().filter(|row| {
                Some(row.sequence) == call.sequence
                    || (call.sequence.is_none() && row.stop == call.stop)
            }) {
                covered.insert((trip.id.to_owned(), trip.date.to_owned(), row.sequence));
            }
            let Some(stop) = stops.iter_mut().find(|stop| stop.id == call.stop) else {
                continue;
            };
            let cancelled = matches!(trip.relationship, 3 | 7);
            let skipped = call.relationship == 1;
            let predicted = if let Some(time) = call.time {
                Some(time)
            } else if version_matches
                && trip.relationship == 0
                && !trip.id.is_empty()
                && !trip.date.is_empty()
                && let (Some(sequence), Some(delay)) = (call.sequence, call.delay)
            {
                schedule
                    .iter()
                    .find(|row| row.stop == call.stop && row.sequence == sequence)
                    .and_then(|row| {
                        scheduled_time(
                            trip.date,
                            if call.departure {
                                &row.departure
                            } else {
                                &row.arrival
                            },
                            delay,
                        )
                    })
            } else {
                None
            };
            let unavailable =
                call.relationship == 2 || (!cancelled && !skipped && predicted.is_none());
            let scheduled = if unavailable && !cancelled && !skipped && version_matches {
                schedule
                    .iter()
                    .find(|row| row.stop == call.stop && Some(row.sequence) == call.sequence)
                    .and_then(|row| scheduled_time(trip.date, &row.departure, 0))
            } else {
                None
            };
            let scheduled = if scheduled.is_some() {
                let service = lookup(&mut query, "trips", trip.id)?
                    .filter(|row| row.len() == 3)
                    .map(|row| row[2].clone());
                if let Some(service) = service {
                    if timetable::active(&service, trip.date, &mut query)? {
                        scheduled
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let at = if cancelled || skipped {
                None
            } else {
                predicted.filter(|_| !unavailable).or(scheduled)
            };
            if at.is_some_and(|at| {
                at.saturating_add(30) < header.timestamp
                    || at > header.timestamp.saturating_add(3 * 3600)
            }) {
                continue;
            }
            let (route, destination, complete) = if let Some(details) = &details {
                details.clone()
            } else {
                let static_trip = if version_matches && !trip.id.is_empty() {
                    lookup(&mut query, "trips", trip.id)?
                } else {
                    None
                };
                let trip_route = static_trip
                    .as_ref()
                    .filter(|row| row.len() >= 2)
                    .map(|row| row[0].as_str())
                    .unwrap_or("");
                // An exact trip-ID join is authoritative for its static route. NTA
                // can publish descriptor route IDs with different separators.
                let static_route = if version_matches && !trip_route.is_empty() {
                    lookup(&mut query, "routes", trip_route)?
                        .and_then(|row| row.first().filter(|s| !s.is_empty()).cloned())
                } else {
                    None
                };
                let descriptor_route =
                    if version_matches && !trip.route.is_empty() && trip.route != trip_route {
                        lookup(&mut query, "routes", trip.route)?
                            .and_then(|row| row.first().filter(|s| !s.is_empty()).cloned())
                    } else {
                        None
                    };
                let conflict = static_route
                    .as_ref()
                    .zip(descriptor_route.as_ref())
                    .is_some_and(|(a, b)| a != b);
                let route = if conflict {
                    None
                } else {
                    static_route.or(descriptor_route)
                };
                let destination = static_trip
                    .as_ref()
                    .filter(|row| row.len() >= 2 && !conflict)
                    .and_then(|row| (!row[1].is_empty()).then(|| row[1].clone()));
                let complete = route.is_some() && destination.is_some();
                let details_row = (
                    route.unwrap_or_else(|| "Route unavailable".into()),
                    destination.unwrap_or_else(|| "Destination unavailable".into()),
                    complete,
                );
                details = Some(details_row.clone());
                details_row
            };
            stop.missing_details |= !complete;
            if !routes.is_empty() && !routes.contains(&route.to_ascii_uppercase()) {
                continue;
            }
            let (status, label) = if cancelled {
                ("cancelled", "Cancelled".into())
            } else if skipped {
                ("skipped", "Does not stop".into())
            } else if unavailable && scheduled.is_some() {
                (
                    "scheduled",
                    format!(
                        "Scheduled {}",
                        clock
                            .format(scheduled.unwrap_or(header.timestamp))
                            .map_err(|_| "invalid scheduled time")?
                    ),
                )
            } else if unavailable {
                ("unavailable", "Time unavailable".into())
            } else {
                let seconds = at
                    .unwrap_or(header.timestamp)
                    .saturating_sub(header.timestamp);
                let minutes = seconds.div_ceil(60);
                (
                    "expected",
                    if minutes == 0 {
                        "Due".into()
                    } else {
                        format!("{minutes} min")
                    },
                )
            };
            // Same trip can repeat a stop; retain distinct calls by event time.
            if stop
                .rows
                .iter()
                .any(|row| row.trip == trip_key && row.at == at && row.status == status)
            {
                continue;
            }
            stop.rows.push(Arrival {
                route,
                destination,
                at,
                label,
                status,
                trip: trip_key.into(),
                observed,
                details: complete,
            });
            stop.rows.sort_by_key(|row| row.at.unwrap_or(u64::MAX));
            stop.rows.truncate(settings.max_arrivals);
        }
    }
    // Fill missing trips from the active timetable, without overriding live,
    // cancelled or skipped calls. All candidates stay in the same three-hour window.
    if version_matches {
        let dates = timetable::service_dates(header.timestamp)?;
        for (trip_id, calls) in &timetables {
            for date in &dates {
                let candidates = calls
                    .iter()
                    .filter_map(|call| {
                        timetable::upcoming(date, &call.departure, header.timestamp)
                            .map(|at| (call, at))
                    })
                    .collect::<Vec<_>>();
                if candidates.is_empty() {
                    continue;
                }
                let Some(trip) = lookup(&mut query, "trips", trip_id)?.filter(|row| row.len() == 3)
                else {
                    continue;
                };
                if !timetable::active(&trip[2], date, &mut query)? {
                    continue;
                }
                let Some(route) =
                    lookup(&mut query, "routes", &trip[0])?.and_then(|row| row.first().cloned())
                else {
                    continue;
                };
                if !routes.is_empty() && !routes.contains(&route.to_ascii_uppercase()) {
                    continue;
                }
                for (call, at) in candidates {
                    if covered.contains(&(trip_id.clone(), date.clone(), call.sequence))
                        || covered.contains(&(trip_id.clone(), String::new(), call.sequence))
                    {
                        continue;
                    }
                    let Some(stop) = stops.iter_mut().find(|stop| stop.id == call.stop) else {
                        continue;
                    };
                    stop.rows.push(Arrival {
                        route: route.clone(),
                        destination: trip[1].clone(),
                        at: Some(at),
                        label: format!(
                            "Scheduled {}",
                            clock.format(at).map_err(|_| "invalid scheduled time")?
                        ),
                        status: "scheduled",
                        trip: format!("{trip_id}|{date}|{}", call.sequence),
                        observed: header.timestamp,
                        details: true,
                    });
                    stop.rows
                        .sort_by_key(|row| (row.at.unwrap_or(u64::MAX), row.status == "scheduled"));
                    stop.rows.truncate(settings.max_arrivals);
                }
            }
        }
    }
    let mut summaries = Vec::new();
    let mut departures = Vec::new();
    let mut board = Vec::new();
    let mut oldest = header.timestamp;
    for stop in &mut stops {
        stop.observed = stop
            .rows
            .iter()
            .map(|row| row.observed)
            .min()
            .unwrap_or(header.timestamp);
        oldest = oldest.min(stop.observed);
        let details =
            version_matches && !stop.missing_details && stop.rows.iter().all(|row| row.details);
        let warning = if details {
            ""
        } else {
            "Some bus details are temporarily unavailable"
        };
        let lines = if stop.rows.is_empty() {
            if !routes.is_empty() && !details {
                "Matching predictions unavailable".into()
            } else if routes.is_empty() {
                "No upcoming departures".into()
            } else {
                "No matching departures".into()
            }
        } else {
            stop.rows
                .iter()
                .map(|row| format!("{} · {} · {}", row.route, row.destination, row.label))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let title = format!("{} · {}", stop.name, stop.number);
        board.push(format!("{title}\n{lines}"));
        summaries.push(BTreeMap::from([("stop-number".into(),json!(stop.number)),("stop-name".into(),json!(stop.name)),("title".into(),json!(title)),("arrivals".into(),json!(lines)),("arrival-count".into(),json!(stop.rows.len())),("details-status".into(),json!(warning)),("updated-label".into(),json!(format!("Updated {} · Europe/Dublin",clock.format(stop.observed).map_err(|_|"invalid feed time")?))),("attribution".into(),json!("NTA · nationaltransport.ie · CC BY 4.0\nData as is; NTA not responsible for errors · formatted"))]));
        for row in &stop.rows {
            departures.push(BTreeMap::from([
                ("stop-number".into(), json!(stop.number)),
                ("stop-name".into(), json!(stop.name)),
                ("route".into(), json!(row.route)),
                ("destination".into(), json!(row.destination)),
                (
                    "expected-at".into(),
                    json!(row.at.filter(|_| row.status != "scheduled")),
                ),
                (
                    "scheduled-at".into(),
                    json!(row.at.filter(|_| row.status == "scheduled")),
                ),
                (
                    "time-label".into(),
                    json!(
                        row.at
                            .map(|at| clock.format(at))
                            .transpose()
                            .map_err(|_| "invalid departure time")?
                    ),
                ),
                ("due-label".into(), json!(row.label)),
                (
                    "due-minutes".into(),
                    json!(
                        row.at
                            .filter(|_| row.status != "scheduled")
                            .map(|at| at.saturating_sub(header.timestamp).div_ceil(60))
                    ),
                ),
                ("status".into(), json!(row.status)),
                ("observed-at".into(), json!(row.observed)),
            ]));
        }
    }
    // Aggregate output remains bounded independently of the national input.
    let board = board.join("\n\n");
    if board.len() > 4096 {
        return Err("stop board exceeds text budget");
    }
    let fields = vec![
        text("stop-number"),
        text("stop-name"),
        text("title"),
        text("arrivals"),
        number("arrival-count"),
        text("details-status"),
        text("updated-label"),
        text("attribution"),
    ];
    let dataset = dataset("stops", fields, summaries);
    let arrivals = crate::dataset(
        "arrivals",
        vec![
            text("stop-number"),
            text("stop-name"),
            text("route"),
            text("destination"),
            number("expected-at"),
            number("scheduled-at"),
            text("time-label"),
            text("due-label"),
            number("due-minutes"),
            text("status"),
            number("observed-at"),
        ],
        departures,
    );
    let overview = crate::dataset(
        "overview",
        vec![text("board"), text("updated-label"), text("attribution")],
        vec![BTreeMap::from([
            ("board".into(), json!(board)),
            (
                "updated-label".into(),
                json!(format!(
                    "Updated {} · Europe/Dublin",
                    clock.format(oldest).map_err(|_| "invalid feed time")?
                )),
            ),
            (
                "attribution".into(),
                json!(
                    "NTA · nationaltransport.ie · CC BY 4.0\nData as is; NTA not responsible for errors · formatted"
                ),
            ),
        ])],
    );
    let publication = Publication {
        dataset,
        observed_at: Some(oldest),
        min_poll_secs: Some(60),
        additional_datasets: vec![arrivals, overview],
        additional_observed_at: BTreeMap::from([
            ("arrivals".into(), oldest),
            ("overview".into(), oldest),
        ]),
    };
    publication
        .validate()
        .map_err(|_| "invalid bus publication")?;
    Ok(publication)
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    fn fetch() -> std::result::Result<(), ()> {
        let settings = paperphase_plugin_sdk::settings().map_err(|_| ())?;
        let (_, numbers, _) = Settings::parse(&settings).map_err(|_| ())?;
        // A missing reference cache must not waste an authenticated feed request.
        stops(&numbers, &mut |table, key| {
            paperphase_plugin_sdk::reference_lookup(table, key)
                .map_err(|_| "reference lookup failed")
        })
        .map_err(|_| ())?;
        let source = paperphase_plugin_sdk::source_url().map_err(|_| ())?;
        let bytes = paperphase_plugin_sdk::http_get_bounded(
            &request_url(&source).map_err(|_| ())?,
            8 * 1024 * 1024,
        )
        .map_err(|_| ())?;
        let publication = convert(&bytes, &settings, |table, key| {
            paperphase_plugin_sdk::reference_lookup(table, key)
                .map_err(|_| "reference lookup failed")
        })
        .map_err(|_| ())?;
        paperphase_plugin_sdk::publish_with_metadata(&publication).map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> SettingsValues {
        serde_json::from_value(json!({"stop_numbers":"2813, 2","routes":"","max_arrivals":6}))
            .unwrap()
    }
    fn reference(table: &str, key: &str) -> Result<Option<Vec<String>>> {
        let data: Value = serde_json::from_str(include_str!("../fixtures/reference.json")).unwrap();
        data.get(table)
            .and_then(|table| table.get(key))
            .map(|row| serde_json::from_value(row.clone()).map_err(|_| "bad fixture"))
            .transpose()
    }
    #[test]
    fn multiple_stops_share_one_feed_and_preserve_due_cancellation_and_unknown_states() {
        let publication = convert(
            include_bytes!("../fixtures/trip-updates.pb"),
            &settings(),
            reference,
        )
        .unwrap();
        assert_eq!(publication.dataset.rows.len(), 2);
        assert_eq!(publication.observed_at, Some(1700000000));
        assert_eq!(publication.additional_observed_at["arrivals"], 1700000000);
        assert_eq!(publication.dataset.rows[0]["stop-name"], "Ranelagh Road");
        let rows = &publication.additional_datasets[0].rows;
        assert_eq!(rows[0]["route"], "46A");
        assert_eq!(rows[0]["due-label"], "Due");
        assert!(
            rows.iter()
                .any(|row| row["status"] == "cancelled" && row["expected-at"].is_null())
        );
        assert!(rows.iter().any(|row| row["status"] == "skipped"));
        assert!(
            rows.iter()
                .any(|row| row["stop-number"] == "2" && row["status"] == "unavailable")
        );
        assert!(
            publication.dataset.rows[0]["details-status"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn stop_lists_are_normalised_bounded_and_routes_filter_after_metadata_lookup() {
        let mut s = settings();
        s.insert("stop_numbers".into(), json!("002813,2813\n2 000002"));
        s.insert("routes".into(), json!("46a"));
        let p = convert(include_bytes!("../fixtures/trip-updates.pb"), &s, reference).unwrap();
        assert_eq!(p.dataset.rows.len(), 2);
        assert_eq!(p.additional_datasets[0].rows.len(), 1);
        assert_eq!(p.additional_datasets[0].rows[0]["route"], "46A");
        for value in ["", "0", "2813,abc", "9999999", "1,2,3,4,5,6,7,8,9"] {
            let mut bad = settings();
            bad.insert("stop_numbers".into(), json!(value));
            assert!(Settings::parse(&bad).is_err());
        }
        let mut s = settings();
        s.insert("stop_numbers".into(), json!("999999"));
        assert!(convert(include_bytes!("../fixtures/empty.pb"), &s, reference).is_err());
    }
    #[test]
    fn empty_predictions_are_success_and_missing_static_details_are_explicit() {
        let p = convert(
            include_bytes!("../fixtures/empty.pb"),
            &settings(),
            reference,
        )
        .unwrap();
        assert!(p.additional_datasets[0].rows.is_empty());
        assert_eq!(p.dataset.rows[0]["arrivals"], "No upcoming departures");
        let p = convert(
            include_bytes!("../fixtures/trip-updates.pb"),
            &settings(),
            |table, key| {
                if table == "trips" {
                    Ok(None)
                } else {
                    reference(table, key)
                }
            },
        )
        .unwrap();
        assert!(
            p.dataset.rows[0]["details-status"]
                .as_str()
                .unwrap()
                .contains("unavailable")
        );
        assert_eq!(
            p.additional_datasets[0].rows[0]["destination"],
            "Destination unavailable"
        );
        let mut filtered = settings();
        filtered.insert("routes".into(), json!("11"));
        let p = convert(
            include_bytes!("../fixtures/trip-updates.pb"),
            &filtered,
            |table, key| {
                if table == "routes" {
                    Ok(None)
                } else {
                    reference(table, key)
                }
            },
        )
        .unwrap();
        assert_eq!(
            p.dataset.rows[0]["arrivals"],
            "Matching predictions unavailable"
        );
        assert!(
            !p.dataset.rows[0]["details-status"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn unrelated_trips_without_ids_do_not_reject_a_national_feed() {
        // Valid GTFS descriptors can identify an unscheduled trip without trip_id.
        let mut bytes = include_bytes!("../fixtures/empty.pb").to_vec();
        bytes.extend_from_slice(&[
            18, 13, 10, 1, b'x', 26, 8, 10, 6, 42, 4, b'r', b'o', b'u', b't',
        ]);
        let publication = convert(&bytes, &settings(), reference).unwrap();
        assert!(publication.additional_datasets[0].rows.is_empty());
    }

    #[test]
    fn delay_predictions_join_the_selected_stop_schedule_and_preserve_missing_states() {
        let bytes = include_bytes!("../fixtures/delay-updates.pb");
        let p = convert(bytes, &settings(), reference).unwrap();
        let row = &p.additional_datasets[0].rows[0];
        assert_eq!(row["status"], "expected");
        assert_eq!(row["expected-at"], 1700000100_u64);
        assert_eq!(row["due-label"], "2 min");
        let p = convert(bytes, &settings(), |table, key| {
            if table == "times" {
                Ok(None)
            } else {
                reference(table, key)
            }
        })
        .unwrap();
        assert_eq!(p.additional_datasets[0].rows[0]["status"], "unavailable");
        let p = convert(bytes, &settings(), |table, key| {
            if table == "feed" {
                Ok(None)
            } else {
                reference(table, key)
            }
        })
        .unwrap();
        assert_eq!(p.additional_datasets[0].rows[0]["status"], "unavailable");
        assert_eq!(
            scheduled_time("20231114", "22:14:00", -46),
            Some(1699999994)
        );
        assert_eq!(
            scheduled_time("20231114", "25:00:00", 0),
            scheduled_time("20231115", "01:00:00", 0)
        );
        // GTFS uses local noon minus 12h: these differ by 23h at spring transition.
        assert_eq!(
            scheduled_time("20240331", "12:00:00", 0).unwrap()
                - scheduled_time("20240330", "12:00:00", 0).unwrap(),
            23 * 3600
        );
        assert!(scheduled_time("bad", "12:00:00", 0).is_none());
        assert!(scheduled_time("20231114", "72:00:00", 0).is_none());
        assert!(scheduled_time("20231114", "12:60:00", 0).is_none());
    }

    #[test]
    fn sparse_updates_propagate_forward_while_no_data_stops_and_skipped_calls_preserve_delays() {
        for (bytes, expected) in [
            (
                &include_bytes!("../fixtures/sparse-updates.pb")[..],
                "expected",
            ),
            (
                &include_bytes!("../fixtures/skipped-updates.pb")[..],
                "expected",
            ),
            (
                &include_bytes!("../fixtures/no-data-updates.pb")[..],
                "unavailable",
            ),
        ] {
            let p = convert(bytes, &settings(), reference).unwrap();
            let rows = &p.additional_datasets[0].rows;
            assert_eq!(rows.len(), 2);
            assert!(rows.iter().all(|row| row["status"] == expected));
            if expected == "expected" {
                assert_eq!(rows[0]["expected-at"], 1700000100_u64);
                assert_eq!(rows[1]["expected-at"], 1700000760_u64);
            } else {
                assert!(rows.iter().all(|row| row["expected-at"].is_null()));
            }
        }
    }

    #[test]
    fn exact_trip_join_resolves_route_id_encoding_differences_and_rejects_known_conflicts() {
        let bytes = include_bytes!("../fixtures/delay-updates.pb");
        let offset = bytes.windows(8).position(|b| b == b"route-11").unwrap();
        let mut changed = bytes.to_vec();
        changed[offset..offset + 8].copy_from_slice(b"route_11");
        let p = convert(&changed, &settings(), reference).unwrap();
        assert_eq!(p.additional_datasets[0].rows[0]["route"], "11");
        assert_eq!(
            p.additional_datasets[0].rows[0]["destination"],
            "City Centre"
        );
        let p = convert(&changed, &settings(), |table, key| {
            if table == "routes" && key == "route_11" {
                Ok(Some(vec!["46A".into()]))
            } else {
                reference(table, key)
            }
        })
        .unwrap();
        assert_eq!(
            p.additional_datasets[0].rows[0]["route"],
            "Route unavailable"
        );
        assert_eq!(
            p.additional_datasets[0].rows[0]["destination"],
            "Destination unavailable"
        );
    }

    fn active_reference(table: &str, key: &str) -> Result<Option<Vec<String>>> {
        if table == "calendar" && key == "daily" {
            Ok(Some(vec![
                "1".into(),
                "1".into(),
                "1".into(),
                "1".into(),
                "1".into(),
                "1".into(),
                "1".into(),
                "20230101".into(),
                "20300101".into(),
            ]))
        } else {
            reference(table, key)
        }
    }
    #[test]
    fn absent_live_trips_use_explicit_scheduled_labels_and_separate_timestamps() {
        let p = convert(
            include_bytes!("../fixtures/empty.pb"),
            &settings(),
            active_reference,
        )
        .unwrap();
        let rows = &p.additional_datasets[0].rows;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["due-label"], "Scheduled 22:14");
        assert_eq!(rows[0]["scheduled-at"], 1700000040_u64);
        assert_eq!(rows[0]["time-label"], "22:14");
        assert!(rows.iter().all(|row| row["status"] == "scheduled"
            && row["expected-at"].is_null()
            && row["due-minutes"].is_null()));
        assert!(
            p.dataset.rows[0]["arrivals"]
                .as_str()
                .unwrap()
                .contains("Scheduled 22:14")
        );
        let unavailable = convert(
            include_bytes!("../fixtures/no-data-updates.pb"),
            &settings(),
            active_reference,
        )
        .unwrap();
        assert!(
            unavailable.additional_datasets[0]
                .rows
                .iter()
                .all(|row| row["status"] == "scheduled")
        );
        let removed = convert(
            include_bytes!("../fixtures/no-data-updates.pb"),
            &settings(),
            |table, key| {
                if table == "exceptions" {
                    Ok(Some(vec!["2".into()]))
                } else {
                    active_reference(table, key)
                }
            },
        )
        .unwrap();
        assert!(
            removed.additional_datasets[0]
                .rows
                .iter()
                .all(|row| row["status"] == "unavailable")
        );
        let p = convert(
            include_bytes!("../fixtures/sparse-updates.pb"),
            &settings(),
            active_reference,
        )
        .unwrap();
        assert_eq!(p.additional_datasets[0].rows.len(), 2);
        assert!(
            p.additional_datasets[0]
                .rows
                .iter()
                .all(|row| row["status"] == "expected" && row["scheduled-at"].is_null())
        );
    }
    #[test]
    fn service_dates_honour_weekdays_validity_and_added_removed_exceptions() {
        let weekday = vec![
            "1".into(),
            "1".into(),
            "1".into(),
            "1".into(),
            "1".into(),
            "0".into(),
            "0".into(),
            "20230101".into(),
            "20300101".into(),
        ];
        let mut query =
            |table: &str, _key: &str| Ok((table == "calendar").then(|| weekday.clone()));
        assert!(timetable::active("daily", "20231114", &mut query).unwrap());
        assert!(!timetable::active("daily", "20231118", &mut query).unwrap());
        assert!(!timetable::active("daily", "20310101", &mut query).unwrap());
        let mut removed = |table: &str, _key: &str| {
            Ok(if table == "exceptions" {
                Some(vec!["2".into()])
            } else {
                Some(weekday.clone())
            })
        };
        assert!(!timetable::active("daily", "20231114", &mut removed).unwrap());
        let mut added = |table: &str, _key: &str| {
            Ok(if table == "exceptions" {
                Some(vec!["1".into()])
            } else {
                None
            })
        };
        assert!(timetable::active("special", "20231118", &mut added).unwrap());
        assert_eq!(
            timetable::upcoming("20231113", "46:14:00", 1700000000),
            Some(1700000040)
        );
        assert!(timetable::upcoming("20231114", "22:00:00", 1700000000).is_none());
        assert!(timetable::upcoming("20231114", "26:00:00", 1700000000).is_none());
    }
    #[test]
    fn cancellations_skips_and_route_filters_take_precedence_over_timetables() {
        let p = convert(
            include_bytes!("../fixtures/cancelled-scheduled.pb"),
            &settings(),
            active_reference,
        )
        .unwrap();
        assert_eq!(p.additional_datasets[0].rows.len(), 2);
        assert!(
            p.additional_datasets[0]
                .rows
                .iter()
                .all(|row| row["status"] == "cancelled" && row["scheduled-at"].is_null())
        );
        let p = convert(
            include_bytes!("../fixtures/skipped-scheduled.pb"),
            &settings(),
            active_reference,
        )
        .unwrap();
        assert!(
            p.additional_datasets[0]
                .rows
                .iter()
                .filter(|row| row["stop-number"] == "2813")
                .all(|row| row["status"] == "skipped")
        );
        let p = convert(
            include_bytes!("../fixtures/sequence-skipped.pb"),
            &settings(),
            active_reference,
        )
        .unwrap();
        assert!(
            p.additional_datasets[0]
                .rows
                .iter()
                .any(|row| row["stop-number"] == "2813" && row["status"] == "skipped")
        );
        let bytes = include_bytes!("../fixtures/skipped-scheduled.pb");
        let offset = bytes
            .windows(12)
            .position(|v| v == b"8220DB002813")
            .unwrap();
        let mut reassigned = bytes.to_vec();
        reassigned[offset..offset + 12].copy_from_slice(b"8220DB999999");
        let p = convert(&reassigned, &settings(), active_reference).unwrap();
        assert!(
            !p.additional_datasets[0]
                .rows
                .iter()
                .any(|row| row["stop-number"] == "2813")
        );
        let mut filtered = settings();
        filtered.insert("routes".into(), json!("46A"));
        let p = convert(
            include_bytes!("../fixtures/empty.pb"),
            &filtered,
            active_reference,
        )
        .unwrap();
        assert!(p.additional_datasets[0].rows.is_empty());
    }
    #[test]
    fn no_boarding_calls_and_excessive_schedule_pages_are_not_displayed() {
        let p = convert(
            include_bytes!("../fixtures/empty.pb"),
            &settings(),
            |table, key| {
                if table == "times" && key == "8220DB002813|0" {
                    Ok(Some(vec![
                        serde_json::to_string(&vec![
                            "trip-delay",
                            "32",
                            "22:14:00",
                            "22:14:00",
                            "1",
                        ])
                        .unwrap(),
                    ]))
                } else {
                    active_reference(table, key)
                }
            },
        )
        .unwrap();
        assert_eq!(p.additional_datasets[0].rows.len(), 1);
        assert_eq!(p.additional_datasets[0].rows[0]["stop-number"], "2");
        assert!(
            convert(
                include_bytes!("../fixtures/empty.pb"),
                &settings(),
                |table, key| if table == "times" {
                    Ok(Some(vec![]))
                } else {
                    active_reference(table, key)
                }
            )
            .is_err()
        );
    }

    #[test]
    fn truncated_overflowing_and_differential_wire_data_is_rejected() {
        let original = include_bytes!("../fixtures/trip-updates.pb");
        for bytes in [
            &b"{}"[..],
            &original[..original.len() - 1],
            &[0xff; 11][..],
            &[0][..],
            &[0x0a, 0xff][..],
        ] {
            assert!(convert(bytes, &settings(), reference).is_err());
        }
        let mut differential = Vec::new();
        differential.extend_from_slice(&[10, 7, 10, 3, b'2', b'.', b'0', 16, 1]);
        assert!(convert(&differential, &settings(), reference).is_err());
        assert!(convert(&vec![0; 8 * 1024 * 1024 + 1], &settings(), reference).is_err());
    }
}
