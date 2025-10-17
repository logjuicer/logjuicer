// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

// This module contains the logic to extract errors from a target.

use anyhow::Result;
use bytes::Bytes;
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use crate::content_get_sources;
use crate::env::TargetEnv;
use crate::indexname_from_source;
use crate::reader::DecompressReader;
use crate::source::LinesIterator;
use crate::LineCounters;
use crate::{config::TargetConfig, unordered::KnownLines};
use logjuicer_report::{Anomaly, AnomalyContext, Content, Epoch, LogReport, Report, Source};

/// A structure to hold the context
struct History(VecDeque<Bytes>);

impl History {
    fn new() -> History {
        History(VecDeque::with_capacity(3))
    }
    fn push(&mut self, b: Bytes) {
        // TODO: use truncate_front(3) when stablizied
        while self.0.len() >= 3 {
            self.0.pop_front();
        }
        self.0.push_back(b);
    }
    fn drain(&mut self) -> Vec<Arc<str>> {
        self.0
            .drain(..)
            .map(|b| logjuicer_iterator::clone_bytes_to_string(&b).unwrap())
            .collect()
    }
    fn last_timestamp(&self) -> Option<crate::timestamps::TS> {
        for bytes in &self.0 {
            if let Ok(s) = std::str::from_utf8(&bytes[..]) {
                if let Some(ts) = crate::timestamps::parse_timestamp(s) {
                    return Some(ts);
                }
            }
        }
        None
    }
}

#[test]
fn test_history() {
    let mut h = History::new();
    for i in ["a", "b", "c", "d"] {
        h.push(i.into())
    }
    assert_eq!(h.drain(), vec!["b".into(), "c".into(), "d".into()]);
    assert_eq!(h.drain(), vec![]);
}

pub struct ErrorsProcessor<'a, R: Read> {
    reader: LinesIterator<R>,
    /// The parser state
    parser: logjuicer_errors::State,
    /// The current anomaly being processed
    current_anomaly: Option<AnomalyContext>,
    /// An error already found, but not returned because the after context of the last anomaly was still being processed.
    next_anomaly: Option<AnomalyContext>,
    /// Previous lines
    history: History,
    /// The list of unique log lines, to avoid searching a line twice.
    skip_lines: Arc<Mutex<Option<KnownLines>>>,
    /// Indicate if run-logjuicer needs to be checked
    is_job_output: bool,
    /// Total lines count
    pub line_count: usize,
    /// Total bytes count
    pub byte_count: usize,
    /// The target user config
    config: &'a TargetConfig,
}

impl<R: Read> Iterator for ErrorsProcessor<'_, R> {
    type Item = Result<AnomalyContext>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_next_error() {
            Ok(Some(e)) => Some(Ok(e)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<'a, R: Read> ErrorsProcessor<'a, R> {
    pub fn new(
        reader: LinesIterator<R>,
        is_job_output: bool,
        skip_lines: Arc<Mutex<Option<KnownLines>>>,
        config: &'a TargetConfig,
    ) -> ErrorsProcessor<'a, R> {
        ErrorsProcessor {
            reader,
            parser: logjuicer_errors::State::new(),
            current_anomaly: None,
            next_anomaly: None,
            history: History::new(),
            is_job_output,
            skip_lines,
            config,
            line_count: 0,
            byte_count: 0,
        }
    }

    fn read_next_error(&mut self) -> Result<Option<AnomalyContext>> {
        // Recover the left-over
        if let Some(a) = self.next_anomaly.take() {
            self.current_anomaly = Some(a);
        }
        for line in self.reader.by_ref() {
            let line = line?;
            self.line_count += 1;
            self.byte_count += line.0.len();
            let raw_str = std::str::from_utf8(&line.0[..])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            let log_pos = line.1;

            // Special check to break when we are processing ourself
            if self.is_job_output && raw_str.contains("TASK [run-logjuicer") {
                break;
            }

            let is_error = match self.parser.parse(raw_str) {
                logjuicer_errors::Result::NoError => false,
                logjuicer_errors::Result::Error => true,
                logjuicer_errors::Result::NeedMore => {
                    // Accumulate the current line in the history
                    self.history.0.push_back(line.0.clone());
                    // If there was an on-going anomaly context, return it now
                    if self.current_anomaly.is_some() {
                        break;
                    }
                    continue;
                }
                logjuicer_errors::Result::CompletedTraceBack => true,
            };

            if self.config.is_ignored_line(raw_str) {
                continue;
            }

            if is_error {
                // Maybe skip lines, ignoring the one with no information, like when ending with 'FAILED! =>'
                if !raw_str.ends_with("FAILED! => ") {
                    if let Some(ref mut skip_lines) = *self.skip_lines.lock().unwrap() {
                        if !skip_lines.insert(&logjuicer_tokenizer::process(raw_str)) {
                            continue;
                        }
                    }
                }
                // Parse timestamp from current line
                let history = &self.history;
                let timestamp = crate::timestamps::parse_timestamp(raw_str)
                    .or_else(|| history.last_timestamp())
                    .and_then(|ts| match ts {
                        crate::timestamps::TS::Full(ts) => Some(ts),
                        crate::timestamps::TS::Time(_) => None,
                    });

                if self.current_anomaly.is_some() {
                    // We need to return the current anomaly now, we'll process this error next time
                    self.next_anomaly = Some(new_error_anomaly(
                        vec![],
                        log_pos,
                        timestamp,
                        raw_str.into(),
                    ));
                    break;
                }
                self.current_anomaly = Some(new_error_anomaly(
                    self.history.drain(),
                    log_pos,
                    timestamp,
                    raw_str.into(),
                ));
            } else if let Some(ref mut anomaly) = self.current_anomaly {
                // Add the line to the after context
                anomaly
                    .after
                    .push(logjuicer_iterator::clone_bytes_to_string(&line.0).unwrap());
                if anomaly.after.len() > 2 {
                    // Stop when the context is completed
                    break;
                }
            } else {
                // Add the line to the history buffer, for the next anomaly before context
                self.history.push(line.0);
            }
        }
        Ok(self.current_anomaly.take())
    }
}

fn new_error_anomaly(
    before: Vec<Arc<str>>,
    pos: usize,
    timestamp: Option<Epoch>,
    line: Arc<str>,
) -> AnomalyContext {
    AnomalyContext {
        before,
        anomaly: Anomaly {
            distance: 0.5,
            pos,
            timestamp,
            line,
        },
        after: Vec::with_capacity(3),
    }
}

#[test]
fn test_errors_processor() {
    let config = &TargetConfig::default();
    let data = std::io::Cursor::new(
        r#"
2025-07-07 - Running a script
2025-07-07 - Traceback (most recent call last):
2025-07-07 -   File "test.py", line 7, in <module>
2025-07-07 -     raise RuntimeError("bam")
2025-07-07 - RuntimeError: bam
2025-07-07 - Something went wrong
"#,
    );
    let skip_lines = Arc::new(Mutex::new(Some(KnownLines::new())));
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(data, false));
    let processor = ErrorsProcessor::new(reader, false, skip_lines, config);
    let mut anomalies = Vec::new();
    for anomaly in processor {
        anomalies.push(anomaly.unwrap())
    }
    let expected = vec![AnomalyContext {
        before: vec![
            "2025-07-07 - Running a script".into(),
            "2025-07-07 - Traceback (most recent call last):".into(),
            "2025-07-07 -   File \"test.py\", line 7, in <module>".into(),
            "2025-07-07 -     raise RuntimeError(\"bam\")".into(),
        ],
        anomaly: Anomaly {
            distance: 0.5,
            pos: 6,
            timestamp: None,
            line: "2025-07-07 - RuntimeError: bam".into(),
        },
        after: vec!["2025-07-07 - Something went wrong".into()],
    }];
    assert_eq!(anomalies, expected);
}

#[test]
fn test_errors_timestamps() {
    let config = &TargetConfig::default();
    let data = std::io::Cursor::new(
        r#"
2025-08-14 13:23:14 message
| fatal: oops
"#,
    );
    let skip_lines = Arc::new(Mutex::new(Some(KnownLines::new())));
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(data, false));
    let processor = ErrorsProcessor::new(reader, false, skip_lines, config);
    let mut anomalies = Vec::new();
    for anomaly in processor {
        anomalies.push(anomaly.unwrap())
    }
    let expected = vec![AnomalyContext {
        before: vec!["2025-08-14 13:23:14 message".into()],
        anomaly: Anomaly {
            distance: 0.5,
            pos: 3,
            timestamp: Some(Epoch(1755177794000)),
            line: "| fatal: oops".into(),
        },
        after: vec![],
    }];
    assert_eq!(anomalies, expected);
}

pub fn get_errors_processor<'a, 'b>(
    env: &'a TargetEnv,
    skip_lines: Arc<Mutex<Option<KnownLines>>>,
    source: &Source,
    reader: DecompressReader<'b>,
) -> Result<ErrorsProcessor<'a, crate::reader::DecompressReader<'b>>> {
    let reader = LinesIterator::new(source, reader)?;
    let is_job_output = if let Some((_, file_name)) = source.as_str().rsplit_once('/') {
        file_name.starts_with("job-output")
    } else {
        false
    };
    env.set_current(source);
    Ok(ErrorsProcessor::new(
        reader,
        is_job_output,
        skip_lines,
        env.config,
    ))
}

/// Create the final report.
#[tracing::instrument(level = "debug", skip(env, counters, reader))]
fn errors_report_source<'a, 'b>(
    env: &'a TargetEnv,
    skip_lines: Arc<Mutex<Option<KnownLines>>>,
    counters: Arc<Mutex<LineCounters>>,
    source: &Source,
    reader: DecompressReader<'b>,
) -> std::result::Result<Option<LogReport>, String> {
    let start_time = Instant::now();
    match get_errors_processor(env, skip_lines, source, reader) {
        Ok(mut processor) => {
            env.set_current(source);
            let mut anomalies = Vec::new();
            for anomaly in processor.by_ref() {
                match anomaly {
                    Ok(anomaly) => anomalies.push(anomaly),
                    Err(err) => return Err(format!("{}", err)),
                }
            }
            let mut counters = counters.lock().unwrap();
            counters.line_count += processor.line_count;
            if !anomalies.is_empty() {
                counters.anomaly_count += anomalies.len();

                Ok(Some(LogReport {
                    test_time: start_time.elapsed(),
                    anomalies,
                    source: source.clone(),
                    index_name: indexname_from_source(source),
                    line_count: processor.line_count,
                    byte_count: processor.byte_count,
                }))
            } else {
                Ok(None)
            }
        }
        Err(err) => Err(format!("{}", err)),
    }
}

/// Create the final report.
#[tracing::instrument(level = "debug", skip(env))]
pub fn errors_report(env: &TargetEnv, target: Content) -> Result<Report> {
    let start_time = Instant::now();
    let created_at = SystemTime::now();
    let read_errors = Mutex::new(Vec::new());
    let counters = Arc::new(Mutex::new(LineCounters::new()));
    let log_reports = Mutex::new(Vec::new());
    let sources = content_get_sources(env, &target)?;
    let skip_lines = Arc::new(Mutex::new(env.new_skip_lines()));

    sources.into_par_iter().for_each(|source| {
        crate::source::with_source(env, source, |source, reader| {
            match reader.and_then(|reader| {
                errors_report_source(env, skip_lines.clone(), counters.clone(), &source, reader)
            }) {
                Ok(Some(lr)) => log_reports.lock().unwrap().push(lr),
                Ok(None) => {}
                Err(err) => read_errors.lock().unwrap().push((source, err.into())),
            }
        })
    });

    let counters = counters.lock().unwrap();
    let read_errors = read_errors.into_inner().unwrap();
    let log_reports = log_reports.into_inner().unwrap();
    Ok(Report {
        created_at,
        run_time: start_time.elapsed(),
        target,
        baselines: vec![],
        log_reports: LogReport::sorted(log_reports),
        index_reports: HashMap::new(),
        unknown_files: HashMap::new(),
        read_errors,
        total_line_count: counters.line_count,
        total_anomaly_count: counters.anomaly_count,
    })
}
