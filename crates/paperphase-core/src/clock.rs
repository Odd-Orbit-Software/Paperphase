use chrono::{DateTime, LocalResult, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::{Result, ValidationError, require};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClockSpec {
    pub timezone: String,
    #[serde(default)]
    pub content: ClockContent,
    #[serde(default)]
    pub hour_cycle: HourCycle,
    #[serde(default)]
    pub date_format: DateFormat,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClockContent {
    #[default]
    Time,
    Date,
    DateTime,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HourCycle {
    #[default]
    TwentyFour,
    Twelve,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    #[default]
    Long,
    Short,
    Iso,
    Weekday,
}

fn error(path: &str, message: &str) -> ValidationError {
    ValidationError {
        path: path.into(),
        message: message.into(),
    }
}
fn zone(name: &str) -> Result<Tz> {
    require(name.len() <= 64, "timezone", "timezone name is too long")?;
    name.parse()
        .map_err(|_| error("timezone", "unknown IANA timezone"))
}
fn instant(now: u64) -> Result<DateTime<Utc>> {
    i64::try_from(now)
        .ok()
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        .ok_or_else(|| {
            error(
                "clock",
                "Unix timestamp is outside the supported calendar range",
            )
        })
}

pub fn timezones() -> Vec<&'static str> {
    chrono_tz::TZ_VARIANTS
        .iter()
        .map(|timezone| timezone.name())
        .collect()
}

impl ClockSpec {
    pub fn validate(&self) -> Result<()> {
        zone(&self.timezone).map(|_| ())
    }

    pub fn format(&self, now: u64) -> Result<String> {
        let local = instant(now)?.with_timezone(&zone(&self.timezone)?);
        let time = local
            .format(match self.hour_cycle {
                HourCycle::TwentyFour => "%H:%M",
                HourCycle::Twelve => "%-I:%M %p",
            })
            .to_string();
        let date = local
            .format(match self.date_format {
                DateFormat::Long => "%A, %-d %B %Y",
                DateFormat::Short => "%-d %b %Y",
                DateFormat::Iso => "%Y-%m-%d",
                DateFormat::Weekday => "%A",
            })
            .to_string();
        Ok(match self.content {
            ClockContent::Time => time,
            ClockContent::Date => date,
            ClockContent::DateTime => format!("{time}\n{date}"),
        })
    }

    pub fn next_change_after(&self, now: u64) -> Result<u64> {
        let timezone = zone(&self.timezone)?;
        let local = instant(now)?.with_timezone(&timezone);
        if self.content != ClockContent::Date {
            // Local minute boundaries can have a non-zero UTC second in historical zones.
            return now
                .checked_add(60 - u64::from(local.second()))
                .ok_or_else(|| error("clock", "timestamp overflow"));
        }
        let mut date = local.date_naive();
        // Usually the first midnight maps directly. Search only when a civil
        // midnight is skipped; this also handles an entirely skipped local date.
        for _ in 0..3 {
            date = date
                .succ_opt()
                .ok_or_else(|| error("clock", "calendar range exceeded"))?;
            for minute in 0..1440 {
                let naive = date
                    .and_hms_opt(minute / 60, minute % 60, 0)
                    .expect("bounded time");
                let candidates = match timezone.from_local_datetime(&naive) {
                    LocalResult::Single(t) => [Some(t), None],
                    LocalResult::Ambiguous(a, b) => [Some(a), Some(b)],
                    LocalResult::None => [None, None],
                };
                if let Some(next) = candidates
                    .into_iter()
                    .flatten()
                    .filter_map(|t| u64::try_from(t.timestamp()).ok())
                    .filter(|t| *t > now)
                    .min()
                {
                    return Ok(next);
                }
            }
        }
        Err(error(
            "clock",
            "could not resolve the next local date boundary",
        ))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshSettings {
    #[serde(default)]
    pub quiet_hours: Option<QuietHours>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuietHours {
    pub timezone: String,
    pub start: String,
    pub end: String,
}

fn wall_minute(value: &str) -> Result<u32> {
    let bytes = value.as_bytes();
    require(
        bytes.len() == 5
            && bytes[2] == b':'
            && [0, 1, 3, 4].iter().all(|i| bytes[*i].is_ascii_digit()),
        "quiet_hours",
        "times must use HH:MM",
    )?;
    let hour = u32::from(bytes[0] - b'0') * 10 + u32::from(bytes[1] - b'0');
    let minute = u32::from(bytes[3] - b'0') * 10 + u32::from(bytes[4] - b'0');
    require(
        hour < 24 && minute < 60,
        "quiet_hours",
        "time is outside 00:00–23:59",
    )?;
    Ok(hour * 60 + minute)
}

impl RefreshSettings {
    pub fn validate(&self) -> Result<()> {
        if let Some(quiet) = &self.quiet_hours {
            zone(&quiet.timezone)?;
            require(
                wall_minute(&quiet.start)? != wall_minute(&quiet.end)?,
                "quiet_hours",
                "start and end must differ; disable quiet hours to allow all-day updates",
            )?;
        }
        Ok(())
    }
    pub fn is_quiet(&self, now: u64) -> Result<bool> {
        let Some(quiet) = &self.quiet_hours else {
            return Ok(false);
        };
        self.validate()?;
        let local = instant(now)?.with_timezone(&zone(&quiet.timezone)?);
        let minute = local.hour() * 60 + local.minute();
        let start = wall_minute(&quiet.start)?;
        let end = wall_minute(&quiet.end)?;
        Ok(if start < end {
            minute >= start && minute < end
        } else {
            minute >= start || minute < end
        })
    }
}
