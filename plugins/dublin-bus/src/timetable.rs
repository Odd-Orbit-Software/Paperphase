//! Selected-stop timetables, paged references and GTFS service-day exceptions.
use super::{Result, Stop, lookup, scheduled_time};
use chrono::{Datelike, TimeZone};
use std::collections::BTreeMap;

pub struct Call {
    pub stop: String,
    pub sequence: u64,
    pub arrival: String,
    pub departure: String,
}
pub fn load<F>(stops: &[Stop], query: &mut F) -> Result<BTreeMap<String, Vec<Call>>>
where
    F: FnMut(&str, &str) -> Result<Option<Vec<String>>>,
{
    let mut trips = BTreeMap::<String, Vec<Call>>::new();
    for stop in stops {
        for page in 0..=64 {
            let Some(rows) = lookup(query, "times", &format!("{}|{page}", stop.id))? else {
                break;
            };
            if page == 64 || rows.len() > 32 {
                return Err("selected stop timetable exceeds budget");
            }
            for row in rows {
                let row: Vec<String> =
                    serde_json::from_str(&row).map_err(|_| "invalid timetable row")?;
                if row.len() != 5 {
                    return Err("invalid timetable columns");
                }
                // Ordinary boarding only; do not advertise drop-off/request-only calls.
                if !matches!(row[4].as_str(), "" | "0") {
                    continue;
                }
                let sequence = row[1].parse().map_err(|_| "invalid timetable sequence")?;
                trips.entry(row[0].clone()).or_default().push(Call {
                    stop: stop.id.clone(),
                    sequence,
                    arrival: row[2].clone(),
                    departure: row[3].clone(),
                });
            }
        }
    }
    Ok(trips)
}
pub fn service_dates(now: u64) -> Result<Vec<String>> {
    let now = i64::try_from(now).map_err(|_| "invalid timetable clock")?;
    let day = chrono_tz::Europe::Dublin
        .timestamp_opt(now, 0)
        .single()
        .ok_or("invalid timetable clock")?
        .date_naive();
    // Up to 71-hour service times; include tomorrow when the window crosses midnight.
    (-2..=1)
        .map(|offset| {
            day.checked_add_signed(chrono::Duration::days(offset))
                .map(|d| d.format("%Y%m%d").to_string())
                .ok_or("invalid service date")
        })
        .collect()
}
pub fn active<F>(service: &str, date: &str, query: &mut F) -> Result<bool>
where
    F: FnMut(&str, &str) -> Result<Option<Vec<String>>>,
{
    if let Some(exception) = lookup(query, "exceptions", &format!("{service}|{date}"))? {
        return match exception.first().map(String::as_str) {
            Some("1") => Ok(true),
            Some("2") => Ok(false),
            _ => Err("invalid timetable exception"),
        };
    }
    let Some(row) = lookup(query, "calendar", service)? else {
        return Ok(false);
    };
    if row.len() != 9 {
        return Err("invalid service calendar");
    }
    let day =
        chrono::NaiveDate::parse_from_str(date, "%Y%m%d").map_err(|_| "invalid service date")?;
    Ok(date >= row[7].as_str()
        && date <= row[8].as_str()
        && row[day.weekday().num_days_from_monday() as usize] == "1")
}
pub fn upcoming(date: &str, time: &str, now: u64) -> Option<u64> {
    scheduled_time(date, time, 0).filter(|at| *at >= now && *at <= now.saturating_add(3 * 3600))
}
