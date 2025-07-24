// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use lazy_static::lazy_static;
use std::io::Read;
use systemd_journal_reader::JournalReader;
use time::UtcDateTime;

pub struct JournalLines<R: Read> {
    journal: JournalReader<R>,
    pos: usize,
}

impl<R: Read> Iterator for JournalLines<R> {
    type Item = std::io::Result<(Bytes, usize)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos += 1;
        match self.journal.next_entry() {
            None => None,
            Some(entry) => {
                let msg: String = entry
                    .fields
                    .get("MESSAGE")
                    .and_then(|msg| {
                        entry
                            .fields
                            .get("SYSLOG_IDENTIFIER")
                            .or(entry.fields.get("_COMM"))
                            .and_then(|sid| {
                                format_ts(entry.realtime as i128)
                                    .map(|ts| format!("{ts} - {sid}: {msg}"))
                            })
                    })
                    .unwrap_or("".into());
                Some(Ok((Bytes::from(msg), self.pos)))
            }
        }
    }
}

fn format_ts(ts: i128) -> Option<String> {
    lazy_static! {
        static ref FMT: Vec<time::format_description::BorrowedFormatItem<'static>> =
            time::format_description::parse(
                "[year]-[month]-[day] [hour]:[minute]:[second],[subsecond digits:3]"
            )
            .unwrap();
    }
    UtcDateTime::from_unix_timestamp_nanos(ts * 1000)
        .ok()
        .and_then(|ts| ts.format(&FMT).ok())
}

impl<R: Read> JournalLines<R> {
    pub fn new(reader: R) -> std::io::Result<JournalLines<R>> {
        let journal = JournalReader::new(reader)?;
        Ok(JournalLines { journal, pos: 0 })
    }
}
