//! Derived agenda from already privacy-filtered connection snapshots. No I/O.
use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use paperphase_core::{Dataset, Field, FieldKind, Snapshot};
use serde_json::{Value, json};

use super::connections::ConnectionStore;

struct Event {
    starts: i64,
    all_day: bool,
    title: String,
    sources: Vec<String>,
    hidden: bool,
}

pub(super) fn snapshots(store: &ConnectionStore) -> Vec<Snapshot> {
    let connections: Vec<_> = store
        .connections
        .iter()
        .filter(|c| c.enabled && c.connector_id == "calendar.google")
        .collect();
    if connections.is_empty() {
        return vec![];
    }
    let mut events: BTreeMap<(String, i64), Event> = BTreeMap::new();
    let mut missing = false;
    let mut failed = false;
    let mut success = u64::MAX;
    let mut observed = u64::MAX;
    let mut stale_after = u64::MAX;
    for connection in &connections {
        let Some(snapshot) = store
            .data
            .get(&connection.id)
            .and_then(|d| d.snapshot.as_ref())
        else {
            missing = true;
            continue;
        };
        failed |= snapshot.last_error.is_some();
        success = success.min(snapshot.last_success_at);
        observed = observed.min(snapshot.observed_at.unwrap_or(snapshot.last_success_at));
        stale_after = stale_after.min(snapshot.stale_after_secs);
        let titles = connection
            .settings
            .get("show_titles")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        for row in &snapshot.dataset.rows {
            let Some(starts) = row.get("start-at").and_then(Value::as_i64) else {
                continue;
            };
            let text = |key| row.get(key).and_then(Value::as_str).unwrap_or("");
            let uid = text("ical-uid");
            // iCalUID identifies the same occurrence copied to multiple calendars.
            // Without it, do not collapse unrelated providers' event IDs.
            let key = if uid.is_empty() {
                format!("{}:{}", connection.id, text("event-id"))
            } else {
                uid.into()
            };
            let event = events.entry((key, starts)).or_insert_with(|| Event {
                starts,
                all_day: row.get("all-day").and_then(Value::as_bool).unwrap_or(false),
                title: text("title").chars().take(160).collect(),
                sources: vec![],
                hidden: false,
            });
            event.hidden |= !titles;
            let source: String = text("source").chars().take(64).collect();
            if !event.sources.contains(&source) {
                event.sources.push(source);
            }
        }
    }
    let mut events: Vec<_> = events.into_values().collect();
    events.sort_by(|a, b| {
        a.starts
            .cmp(&b.starts)
            .then(a.title.cmp(&b.title))
            .then(a.sources.cmp(&b.sources))
    });
    connections
        .iter()
        .map(|connection| {
            let timezone = connection
                .settings
                .get("timezone")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<chrono_tz::Tz>().ok())
                .unwrap_or(chrono_tz::UTC);
            let maximum = connection
                .settings
                .get("max_events")
                .and_then(Value::as_u64)
                .unwrap_or(8)
                .clamp(1, 12) as usize;
            let mut lines = vec![];
            if missing || failed {
                lines.push("Some calendars unavailable; showing cached events".into());
            }
            for event in events.iter().take(maximum) {
                let Some(local) = Utc
                    .timestamp_opt(event.starts, 0)
                    .single()
                    .map(|d| d.with_timezone(&timezone))
                else {
                    continue;
                };
                let when = if event.all_day {
                    format!("{} · All day", local.format("%a %d %b"))
                } else {
                    local.format("%a %d %b · %H:%M").to_string()
                };
                let source: String = event.sources.join(", ").chars().take(96).collect();
                let title = if event.hidden { "Busy" } else { &event.title };
                lines.push(format!("{when}  {title} [{source}]"));
            }
            if lines.is_empty() {
                lines.push("No upcoming events".into());
            }
            let summary: String = lines.join("\n").chars().take(1024).collect();
            Snapshot {
                connection_id: connection.id.clone(),
                dataset: Dataset {
                    schema_version: 1,
                    id: "combined-agenda".into(),
                    fields: ["title", "summary"]
                        .map(|id| Field {
                            id: id.into(),
                            kind: FieldKind::Text,
                            unit: None,
                        })
                        .into(),
                    rows: vec![BTreeMap::from([
                        ("title".into(), json!("Combined agenda")),
                        ("summary".into(), json!(summary)),
                    ])],
                },
                last_success_at: if success == u64::MAX { 0 } else { success },
                observed_at: Some(if observed == u64::MAX { 0 } else { observed }),
                stale_after_secs: if stale_after == u64::MAX {
                    900
                } else {
                    stale_after
                },
                last_error: (missing || failed)
                    .then(|| "One or more source calendars are unavailable".into()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::fetching::ConnectionData;
    use super::*;
    use paperphase_core::ConnectionConfig;

    #[test]
    fn combines_accounts_deduplicates_occurrences_and_preserves_privacy_and_age() {
        let mut store = ConnectionStore::default();
        for (id, label, titles, success) in
            [("personal", "Home", true, 100), ("work", "Work", false, 90)]
        {
            store.connections.push(ConnectionConfig {
                id: id.into(),
                connector_id: "calendar.google".into(),
                label: label.into(),
                enabled: true,
                poll_interval_secs: 300,
                settings: BTreeMap::from([
                    ("show_titles".into(), json!(titles)),
                    ("timezone".into(), json!("Europe/Dublin")),
                    ("max_events".into(), json!(8)),
                ]),
            });
            let snapshot = Snapshot {
                connection_id: id.into(),
                dataset: Dataset {
                    schema_version: 1,
                    id: "events".into(),
                    fields: vec![],
                    rows: vec![BTreeMap::from([
                        ("ical-uid".into(), json!("same-occurrence")),
                        ("event-id".into(), json!(id)),
                        ("start-at".into(), json!(1794700800)),
                        ("all-day".into(), json!(false)),
                        ("title".into(), json!("Private meeting")),
                        ("source".into(), json!(label)),
                    ])],
                },
                last_success_at: success,
                observed_at: Some(success),
                stale_after_secs: 900,
                last_error: None,
            };
            store.data.insert(
                id.into(),
                ConnectionData {
                    snapshot: Some(snapshot),
                    ..ConnectionData::default()
                },
            );
        }
        let combined = snapshots(&store);
        assert_eq!(combined.len(), 2);
        let text = combined[0].dataset.rows[0]["summary"].as_str().unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(
            text.contains("Busy")
                && text.contains("Home, Work")
                && !text.contains("Private meeting")
        );
        assert_eq!(combined[0].last_success_at, 90);
        assert_eq!(combined[0].observed_at, Some(90));
        store.data.remove("work");
        let combined = snapshots(&store);
        assert!(combined[0].last_error.is_some());
        assert!(
            combined[0].dataset.rows[0]["summary"]
                .as_str()
                .unwrap()
                .starts_with("Some calendars unavailable")
        );
        store.connections[1].enabled = false;
        let combined = snapshots(&store);
        assert_eq!(combined.len(), 1);
        assert!(combined[0].last_error.is_none());
    }
}
