use chrono::{TimeZone, Utc};
use paperphase_core::{
    ClockContent, ClockSpec, DateFormat, HourCycle, QuietHours, RefreshSettings,
};

fn at(value: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp()
        .try_into()
        .unwrap()
}
fn clock(zone: &str, content: ClockContent) -> ClockSpec {
    ClockSpec {
        timezone: zone.into(),
        content,
        hour_cycle: HourCycle::TwentyFour,
        date_format: DateFormat::Iso,
    }
}
#[test]
fn clock_formats_midnight_noon_dates_and_account_independent_timezones() {
    let mut spec = clock("Europe/Dublin", ClockContent::DateTime);
    assert_eq!(
        spec.format(at("2026-07-01T23:00:00Z")).unwrap(),
        "00:00\n2026-07-02"
    );
    spec.hour_cycle = HourCycle::Twelve;
    assert_eq!(
        spec.format(at("2026-07-01T23:00:00Z")).unwrap(),
        "12:00 AM\n2026-07-02"
    );
    assert_eq!(
        spec.format(at("2026-07-02T11:00:00Z")).unwrap(),
        "12:00 PM\n2026-07-02"
    );
    spec.timezone = "Asia/Kolkata".into();
    assert_eq!(
        spec.format(at("2026-07-02T00:00:00Z")).unwrap(),
        "5:30 AM\n2026-07-02"
    );
    spec.content = ClockContent::Date;
    spec.date_format = DateFormat::Long;
    assert_eq!(
        spec.format(at("2026-11-10T00:00:00Z")).unwrap(),
        "Tuesday, 10 November 2026"
    );
    spec.date_format = DateFormat::Short;
    assert_eq!(
        spec.format(at("2026-11-10T00:00:00Z")).unwrap(),
        "10 Nov 2026"
    );
    spec.date_format = DateFormat::Weekday;
    assert_eq!(spec.format(at("2026-11-10T00:00:00Z")).unwrap(), "Tuesday");
}

#[test]
fn date_boundaries_follow_dst_and_skipped_civil_dates() {
    let spec = clock("Europe/Dublin", ClockContent::Date);
    let spring = at("2026-03-29T00:00:00Z");
    let autumn = at("2026-10-24T23:00:00Z");
    assert_eq!(spec.next_change_after(spring).unwrap() - spring, 23 * 3600);
    assert_eq!(spec.next_change_after(autumn).unwrap() - autumn, 25 * 3600);
    let skipped = clock("Pacific/Apia", ClockContent::Date);
    let before = at("2011-12-30T09:59:40Z");
    let next = skipped.next_change_after(before).unwrap();
    assert_eq!(next, at("2011-12-30T10:00:00Z"));
    assert_eq!(skipped.format(next).unwrap(), "2011-12-31");
    let midnight_gap = clock("America/Sao_Paulo", ClockContent::Date);
    assert_eq!(
        midnight_gap
            .next_change_after(at("2018-11-03T12:00:00Z"))
            .unwrap(),
        at("2018-11-04T03:00:00Z")
    );
}

#[test]
fn minute_boundaries_and_bad_clock_configuration_are_checked() {
    let spec = clock("Europe/Dublin", ClockContent::Time);
    let now = at("2026-10-25T00:59:59Z");
    assert_eq!(spec.next_change_after(now).unwrap(), now + 1);
    assert_eq!(spec.format(now).unwrap(), "01:59");
    assert_eq!(spec.format(now + 1).unwrap(), "01:00");
    assert!(clock("Not/AZone", ClockContent::Time).validate().is_err());
    assert!(spec.format(u64::MAX).is_err());
    assert!(spec.next_change_after(u64::MAX).is_err());
    let historical = clock("Africa/Monrovia", ClockContent::Time);
    let now = Utc
        .with_ymd_and_hms(1971, 1, 1, 0, 0, 0)
        .unwrap()
        .timestamp() as u64;
    assert_eq!(historical.next_change_after(now).unwrap(), now + 30);
}

#[test]
fn quiet_hours_are_half_open_and_follow_repeated_local_hours() {
    let overnight = RefreshSettings {
        quiet_hours: Some(QuietHours {
            timezone: "Europe/Dublin".into(),
            start: "22:00".into(),
            end: "07:00".into(),
        }),
        ..RefreshSettings::default()
    };
    for (time, expected) in [
        ("2026-11-10T21:59:59Z", false),
        ("2026-11-10T22:00:00Z", true),
        ("2026-11-11T06:59:59Z", true),
        ("2026-11-11T07:00:00Z", false),
        ("2026-10-25T00:30:00Z", true),
        ("2026-10-25T01:30:00Z", true),
    ] {
        assert_eq!(overnight.is_quiet(at(time)).unwrap(), expected, "{time}");
    }
    let mut daytime = overnight;
    let quiet = daytime.quiet_hours.as_mut().unwrap();
    quiet.start = "12:00".into();
    quiet.end = "13:00".into();
    assert!(!daytime.is_quiet(at("2026-11-10T11:59:00Z")).unwrap());
    assert!(daytime.is_quiet(at("2026-11-10T12:00:00Z")).unwrap());
    assert!(!daytime.is_quiet(at("2026-11-10T13:00:00Z")).unwrap());
    for invalid in ["", "7:00", "24:00", "12:60", "ab:cd", "13:00"] {
        daytime.quiet_hours.as_mut().unwrap().start = invalid.into();
        assert!(daytime.validate().is_err(), "{invalid}");
    }
    assert!(!RefreshSettings::default().is_quiet(u64::MAX).unwrap());
}
