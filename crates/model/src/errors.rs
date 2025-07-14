// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

// This module contains the logic to extract errors from a target.

use anyhow::Result;
use bytes::Bytes;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use crate::content_get_sources;
use crate::env::TargetEnv;
use crate::files::file_open;
use crate::indexname_from_source;
use crate::urls::url_open;
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
    fn drain(&mut self) -> Vec<Rc<str>> {
        self.0
            .drain(..)
            .map(|b| logjuicer_iterator::clone_bytes_to_string(&b).unwrap())
            .collect()
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
    reader: logjuicer_iterator::BytesLines<R>,
    /// The parser state
    parser: logjuicer_errors::State,
    /// The current anomaly being processed
    current_anomaly: Option<AnomalyContext>,
    /// An error already found, but not returned because the after context of the last anomaly was still being processed.
    next_anomaly: Option<AnomalyContext>,
    /// Previous lines
    history: History,
    /// The list of unique log lines, to avoid searching a line twice.
    skip_lines: &'a mut Option<KnownLines>,
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
        read: R,
        is_json: bool,
        is_job_output: bool,
        skip_lines: &'a mut Option<KnownLines>,
        config: &'a TargetConfig,
    ) -> ErrorsProcessor<'a, R> {
        ErrorsProcessor {
            reader: logjuicer_iterator::BytesLines::new(read, is_json),
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
                if let Some(skip_lines) = self.skip_lines {
                    if !skip_lines.insert(&logjuicer_tokenizer::process(raw_str)) {
                        continue;
                    }
                }
                // Parse timestamp from current line
                let timestamp =
                    crate::timestamps::parse_timestamp(raw_str).and_then(|ts| match ts {
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
    before: Vec<Rc<str>>,
    pos: usize,
    timestamp: Option<Epoch>,
    line: Rc<str>,
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
    let mut skip_lines = Some(KnownLines::new());
    let processor = ErrorsProcessor::new(data, false, false, &mut skip_lines, config);
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

pub fn get_errors_processor<'a>(
    env: &'a TargetEnv,
    skip_lines: &'a mut Option<KnownLines>,
    source: &Source,
) -> Result<ErrorsProcessor<'a, crate::reader::DecompressReader>> {
    let fp = match source {
        Source::Local(_, path_buf) => file_open(path_buf.as_path()),
        Source::Remote(_, url) => url_open(env.gl, url),
    }?;
    let is_job_output = if let Some((_, file_name)) = source.as_str().rsplit_once('/') {
        file_name.starts_with("job-output")
    } else {
        false
    };
    env.set_current(source);
    Ok(ErrorsProcessor::new(
        fp,
        source.is_json(),
        is_job_output,
        skip_lines,
        env.config,
    ))
}

/// Create the final report.
#[tracing::instrument(level = "debug", skip(env, counters))]
fn errors_report_source<'a>(
    env: &'a TargetEnv,
    skip_lines: &'a mut Option<KnownLines>,
    counters: &mut LineCounters,
    source: &Source,
) -> std::result::Result<Option<LogReport>, String> {
    let start_time = Instant::now();
    match get_errors_processor(env, skip_lines, source) {
        Ok(mut processor) => {
            env.set_current(source);
            let mut anomalies = Vec::new();
            for anomaly in processor.by_ref() {
                match anomaly {
                    Ok(anomaly) => anomalies.push(anomaly),
                    Err(err) => return Err(format!("{}", err)),
                }
            }
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
    let mut read_errors = Vec::new();
    let mut counters = LineCounters::new();
    let mut log_reports = Vec::new();
    let sources = content_get_sources(env, &target)?;
    let mut skip_lines = env.new_skip_lines();
    // TODO: use threadpool
    for source in &sources {
        match errors_report_source(env, &mut skip_lines, &mut counters, source) {
            Ok(Some(lr)) => log_reports.push(lr),
            Ok(None) => {}
            Err(err) => read_errors.push((source.clone(), err.into())),
        }
    }

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
