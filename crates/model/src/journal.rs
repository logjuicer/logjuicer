// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use lazy_static::lazy_static;
use std::io::{Read, Result};
use systemd_journal_reader::JournalReader;
use time::UtcDateTime;

pub struct JournalLines<R: Read> {
    journal: JournalReader<R>,
    remaining: Option<(usize, Bytes)>,
    pos: usize,
}

impl<R: Read> Iterator for JournalLines<R> {
    type Item = Result<(Bytes, usize)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos += 1;
        if let Some(remaining) = self.remaining.take() {
            self.next_remaining(false, remaining.0, remaining.1)
        } else {
            self.next_entry()
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

fn is_multiline(mut line: Bytes) -> std::result::Result<Bytes, (Bytes, Bytes)> {
    if let Some(pos) = line.iter().position(|c| c == &b'\n') {
        let rest = line.split_off(pos);
        Err((line, rest.slice(1..rest.len())))
    } else {
        Ok(line)
    }
}

impl<R: Read> JournalLines<R> {
    pub fn new(reader: R) -> Result<JournalLines<R>> {
        let journal = JournalReader::new(reader)?;
        Ok(JournalLines {
            journal,
            pos: 0,
            remaining: None,
        })
    }
    fn next_remaining(
        &mut self,
        first: bool,
        prefix: usize,
        remaining: Bytes,
    ) -> Option<Result<(Bytes, usize)>> {
        let line = match is_multiline(remaining) {
            Ok(line) => {
                self.remaining = None;
                line
            }
            Err((line, remaining)) => {
                self.remaining = Some((prefix, remaining));
                line
            }
        };
        let line = if first {
            line
        } else {
            let mut buf = Vec::with_capacity(prefix + line.len());
            buf.resize(prefix, b' ');
            buf.extend_from_slice(&line);
            Bytes::from(buf)
        };
        Some(Ok((line, self.pos)))
    }
    fn next_entry(&mut self) -> Option<Result<(Bytes, usize)>> {
        match self.journal.next_entry() {
            None => None,
            Some(entry) => {
                let (prefix, msg): (usize, String) = entry
                    .fields
                    .get("MESSAGE")
                    .and_then(|msg| {
                        let msg = msg.trim_end_matches('\n');
                        entry
                            .fields
                            .get("SYSLOG_IDENTIFIER")
                            .or(entry.fields.get("_COMM"))
                            .and_then(|sid| {
                                format_ts(entry.realtime as i128).map(|ts| {
                                    (ts.len() + sid.len() + 5, format!("{ts} - {sid}: {msg}"))
                                })
                            })
                    })
                    .unwrap_or((0, "".into()));
                self.next_remaining(true, prefix, Bytes::from(msg))
            }
        }
    }
}
