// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

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

struct History {
    lines: VecDeque<Bytes>,
}

impl History {
    fn new() -> History {
        History {
            lines: VecDeque::with_capacity(3),
        }
    }
    fn push(&mut self, b: Bytes) {
        self.lines.truncate_front(3);
        self.lines.push_back(b);
    }
    fn drain(&mut self) -> Vec<Rc<str>> {
        self.lines
            .drain(..)
            .map(|b| logjuicer_iterator::clone_bytes_to_string(&b).unwrap())
            .collect()
    }
}

pub struct ErrorsProcessor<'a, R: Read> {
    reader: logjuicer_iterator::BytesLines<R>,
    /// The current anomaly being processed
    current_anomaly: Option<AnomalyContext>,
    /// An error already found, but not returned because the after context of the last anomaly was still being processed.
    last_error: Option<(Rc<str>, usize, Option<Epoch>)>,
    /// Previous lines
    history: History,
    /// The list of unique log lines, to avoid searching a line twice.
    skip_lines: &'a mut Option<KnownLines>,
    /// Indicate if run-logjuicer needs to be checked
    is_job_output: bool,
    parser: logjuicer_errors::State,
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
            current_anomaly: None,
            last_error: None,
            history: History::new(),
            is_job_output,
            skip_lines,
            config,
            parser: logjuicer_errors::State::new(),
            line_count: 0,
            byte_count: 0,
        }
    }

    fn read_next_error(&mut self) -> Result<Option<AnomalyContext>> {
        // Handle the left-over
        if let Some(err) = self.last_error.take() {
            self.current_anomaly = Some(AnomalyContext {
                before: vec![],
                anomaly: Anomaly {
                    distance: 0.5,
                    pos: err.1,
                    timestamp: err.2,
                    line: err.0,
                },
                after: Vec::with_capacity(3),
            })
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
                    self.history.lines.push_back(line.0.clone());
                    // If there was an on-going anomaly context, return it now
                    if self.current_anomaly.is_some() {
                        break;
                    }
                    false
                }
                logjuicer_errors::Result::CompletedTraceBack => true,
            };

            if self.config.is_ignored_line(raw_str) {
                continue;
            }

            if is_error {
                let tokens = logjuicer_tokenizer::process(raw_str);
                let process_line = if let Some(skip_lines) = self.skip_lines {
                    skip_lines.insert(&tokens)
                } else {
                    true
                };
                if process_line {
                    // Parse timestamp from current line
                    let timestamp =
                        crate::timestamps::parse_timestamp(raw_str).and_then(|ts| match ts {
                            crate::timestamps::TS::Full(ts) => Some(ts),
                            crate::timestamps::TS::Time(_) => None,
                        });

                    if self.current_anomaly.is_some() {
                        // We need to return the current anomaly now, we'll process this error next time
                        self.last_error = Some((raw_str.into(), log_pos, timestamp));
                        break;
                    }
                    self.current_anomaly = Some(AnomalyContext {
                        before: self.history.drain(),
                        anomaly: Anomaly {
                            distance: 0.5,
                            pos: log_pos,
                            timestamp,
                            line: raw_str.into(),
                        },
                        after: Vec::with_capacity(3),
                    });
                }
            } else if let Some(ref mut anomaly) = self.current_anomaly {
                anomaly
                    .after
                    .push(logjuicer_iterator::clone_bytes_to_string(&line.0).unwrap());
                if anomaly.after.len() > 2 {
                    break;
                }
            } else {
                self.history.push(line.0);
            }
        }
        Ok(self.current_anomaly.take())
    }
}

pub fn get_errors_processor<'a>(
    env: &'a TargetEnv,
    source: &Source,
    skip_lines: &'a mut Option<KnownLines>,
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

fn errors_report_source(
    env: &TargetEnv,
    counters: &mut LineCounters,
    source: &Source,
) -> std::result::Result<Option<LogReport>, String> {
    let start_time = Instant::now();
    match get_errors_processor(env, source, &mut env.new_skip_lines()) {
        Ok(mut processor) => {
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
    // TODO: use threadpool
    for source in &sources {
        match errors_report_source(env, &mut counters, source) {
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
