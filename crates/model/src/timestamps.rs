// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

use chrono::Timelike;
use chrono::{NaiveDateTime, NaiveTime};

#[derive(Clone, Debug, PartialEq)]
pub enum TS {
    Full(Epoch),
    Time(u64),
}

use logjuicer_report::Epoch;
use TS::*;

pub fn parse_timestamp(line: &str) -> Option<TS> {
    if let Some(stripped) = line.strip_prefix("{\"date\":") {
        NaiveDateTime::parse_and_remainder(stripped, "%s.%3f")
            .map(|(ndt, _)| Full(Epoch(ndt.timestamp_millis() as u64)))
            .ok()
    } else {
        NaiveDateTime::parse_and_remainder(line, "%F %T,%3f")
            .or_else(|_| NaiveDateTime::parse_and_remainder(line, "%FT%T"))
            .or_else(|_| NaiveDateTime::parse_and_remainder(line, "%F %T.%3f"))
            .or_else(|_| NaiveDateTime::parse_and_remainder(line, "[%Y/%m/%d %T]"))
            .map(|(ndt, _)| Full(Epoch(ndt.timestamp_millis() as u64)))
            .or_else(|_| {
                NaiveTime::parse_and_remainder(line.get(6..).unwrap_or(""), "%T.%3f")
                    .or_else(|_| NaiveTime::parse_and_remainder(line, "%b %d %T "))
                    .map(|(nt, _)| {
                        Time(
                            (nt.num_seconds_from_midnight() as u64) * 1_000
                                + ((nt.nanosecond() / 1_000_000) as u64),
                        )
                    })
            })
            .ok()
    }
}

#[test]
fn test_timestamp() {
    assert_eq!(parse_timestamp("Feb 27 11:06:45 "), Some(Time(40005000)));
    assert_eq!(
        parse_timestamp("2024-02-27T15:58:33Z "),
        Some(Full(Epoch(1709049513000)))
    );
    assert_eq!(
        parse_timestamp("{\"date\":1708419555.859087,"),
        Some(Full(Epoch(1708419555859)))
    );
    assert_eq!(
        parse_timestamp("[2024/02/20 09:13:35]"),
        Some(Full(Epoch(1708420415000)))
    );
    assert_eq!(
        parse_timestamp("2024-02-20 09:15:54.012305"),
        Some(Full(Epoch(1708420554012)))
    );
    assert_eq!(
        parse_timestamp("2024-02-20 09:06:57,036 INFO"),
        Some(Full(Epoch(1708420017036)))
    );
    assert_eq!(
        parse_timestamp("I0220 08:45:08.004309  "),
        Some(Time(31508004))
    )
}

const HOUR: u64 = 3_600_000;
const DAY: u64 = HOUR * 24;

// Set date to time using a previously known datetime
pub fn set_date(date_time: Epoch, time: u64) -> Epoch {
    let known_time = date_time.0 % DAY;
    let known_date = date_time.0 / DAY * DAY;
    let diff = known_time.abs_diff(time);
    Epoch(if known_time > time {
        if diff > HOUR * 12 {
            // 2024-01-01T23:00:00 and 01:01:01, the time is tomorrow
            known_date + DAY + time
        } else {
            // 2024-01-01T23:00:00 and 22:01:01, the time is today, it happened before the known date
            known_date + time
        }
    } else if diff > HOUR * 12 {
        // 2024-01-01T01:01:01 and 23:00:00, the time is yesterday
        known_date - DAY + time
    } else {
        // 2024-01-01T01:01:01 and 04:00:00
        known_date + time
    })
}

#[test]
fn test_set_date() {
    fn get_datetime(date_str: &str, time_str: &str) -> String {
        let date = match parse_timestamp(date_str) {
            Some(Full(d)) => d,
            _ => panic!("bad date"),
        };
        let time = match parse_timestamp(time_str) {
            Some(Time(t)) => t,
            _ => panic!("bad time"),
        };
        let epoch = set_date(date, time);
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(epoch.0 as i64)
            .expect("invalid timestamp")
            .to_string()
    }
    assert_eq!(
        get_datetime("2024-02-27 11:05:43.333901", "Feb 27 10:41:36"),
        "2024-02-27 10:41:36 UTC".to_string()
    );
    assert_eq!(
        get_datetime("2000-01-01 23:00:00.000", "I0000 01:00:00.000"),
        "2000-01-02 01:00:00 UTC".to_string()
    );
    assert_eq!(
        get_datetime("2000-01-01 23:00:00.000", "I0000 18:00:00.000"),
        "2000-01-01 18:00:00 UTC".to_string()
    );
    assert_eq!(
        get_datetime("2000-01-01 01:00:00.000", "I0000 18:00:00.000"),
        "1999-12-31 18:00:00 UTC".to_string()
    );
    assert_eq!(
        get_datetime("2000-01-01 01:00:00.000", "I0000 05:00:00.000"),
        "2000-01-01 05:00:00 UTC".to_string()
    );
}
