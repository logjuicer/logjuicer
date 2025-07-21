// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use lazy_static::lazy_static;
use systemd::journal::{Journal, OpenFilesOptions};
use time::UtcDateTime;

pub struct JournalLines {
    journal: Journal,
    pos: usize,
}

impl Iterator for JournalLines {
    type Item = std::io::Result<(Bytes, usize)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos += 1;
        match self.journal.next_entry() {
            Err(e) => Some(Err(e)),
            Ok(None) => None,
            Ok(Some(entry)) => {
                let msg: String = entry
                    .get("MESSAGE")
                    .and_then(|msg| {
                        entry
                            .get("SYSLOG_IDENTIFIER")
                            .or(entry.get("_COMM"))
                            .and_then(|sid| {
                                entry
                                    .get("_SOURCE_REALTIME_TIMESTAMP")
                                    .or(entry.get("__REALTIME_TIMESTAMP"))
                                    .and_then(|ts| {
                                        parse_ts(ts).map(|ts| format!("{ts} - {sid}: {msg}"))
                                    })
                            })
                    })
                    .unwrap_or("".into());
                Some(Ok((Bytes::from(msg), self.pos)))
            }
        }
    }
}

fn parse_ts(ts: &str) -> Option<String> {
    lazy_static! {
        static ref FMT: Vec<time::format_description::BorrowedFormatItem<'static>> =
            time::format_description::parse(
                "[year]-[month]-[day] [hour]:[minute]:[second],[subsecond digits:3]"
            )
            .unwrap();
    }
    ts.parse::<i128>()
        .ok()
        .and_then(|ts| UtcDateTime::from_unix_timestamp_nanos(ts * 1000).ok())
        .and_then(|ts| ts.format(&FMT).ok())
}

impl JournalLines {
    pub fn new(path: &std::path::Path) -> std::io::Result<JournalLines> {
        let fp = path.to_string_lossy();
        let fps: &str = &fp;
        let journal = OpenFilesOptions::default().open_files([fps])?;
        Ok(JournalLines { journal, pos: 0 })
    }
}
