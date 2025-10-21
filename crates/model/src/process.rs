// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides the core utilities to use logjuicer-index with Read objects.

use anyhow::Result;
use std::io::Read;
use std::sync::Arc;
use std::{collections::VecDeque, sync::Mutex};

use crate::config::TargetConfig;
use crate::source::LinesIterator;
use crate::timestamps::TS;
use crate::unordered::KnownLines;
use logjuicer_index::traits::*;
use logjuicer_iterator::LogLine;
use logjuicer_report::{Anomaly, AnomalyContext, Epoch};

// The minimum distance for a line to be considered anomalous
pub(crate) const THRESHOLD: logjuicer_index::F = 0.3;
// The size of the before/after context to include
pub(crate) const CTX_LENGTH: usize = 3;
// The size of the before context when it touches the previous anomaly
const BETWEEN_CTX_LENGTH: usize = 12;
// The matrix size to compute distances in batch
const CHUNK_SIZE: usize = 512;

/// Helper struct to manage indexing multiples readers.
pub struct IndexTrainer<IB: IndexBuilder> {
    builder: IB,
    skip_lines: KnownLines,
    pub line_count: usize,
    pub byte_count: usize,
}

impl<IB> IndexTrainer<IB>
where
    IB: IndexBuilder,
{
    pub fn new(builder: IB) -> IndexTrainer<IB> {
        Self {
            builder,
            skip_lines: KnownLines::new(),
            line_count: 0,
            byte_count: 0,
        }
    }

    /// Index a single reader
    pub fn single<R: Read>(
        builder: IB,
        is_json: bool,
        config: &TargetConfig,
        read: R,
    ) -> Result<IB::Reader> {
        let mut trainer = IndexTrainer::new(builder);
        let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(read, is_json));
        trainer.add(config, reader)?;
        Ok(trainer.build())
    }

    #[tracing::instrument(level = "debug", name = "Trainer::add", skip_all)]
    pub fn add<R: Read>(&mut self, config: &TargetConfig, reader: LinesIterator<R>) -> Result<()> {
        for line in reader {
            let line = line?;
            let raw_str = std::str::from_utf8(&line.0[..])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            self.line_count += 1;
            self.byte_count += line.0.len();

            if config.is_ignored_line(raw_str) {
                continue;
            }

            let tokens = logjuicer_tokenizer::process(raw_str);

            if self.skip_lines.insert(&tokens) {
                self.builder.add(&tokens);
            }
        }
        tracing::debug!(skip_lines = self.skip_lines.len(), "added one source");
        Ok(())
    }

    pub fn build(self) -> IB::Reader {
        self.builder.build()
    }
}

/// Helper struct to manage the log lines and the unique tokenized lines.
/// The goal is to perform the index search on unique lines, while keeping a
/// buffer of the raw line to manage the surrounding context.
pub struct ChunkProcessor<'a, IR: IndexReader, R: Read> {
    reader: LinesIterator<R>,
    index: &'a IR,
    /// The raw log line with their global position
    buffer: Vec<(logjuicer_iterator::LogLine, usize)>,
    /// The target tokenized lines
    targets: Vec<String>,
    /// The target positions
    targets_coord: Vec<usize>,
    /// The very last lines of the current buffer that could be the prev context of the next chunk
    left_overs: Vec<Arc<str>>,
    /// The current anomaly being processed
    current_anomaly: Option<AnomalyContext>,
    /// The list of anomalies recently found.
    anomalies: VecDeque<AnomalyContext>,
    /// The list of unique log lines, to avoid searching a line twice.
    skip_lines: (&'a mut Option<KnownLines>, Arc<Mutex<Option<KnownLines>>>),
    /// The current line coordinate.
    coord: usize,
    /// Total lines count
    pub line_count: usize,
    /// Total bytes count
    pub byte_count: usize,
    /// Indicate if run-logjuicer needs to be checked
    is_job_output: bool,
    /// Global full date time to adjust logline which only has the time
    gl_date: Option<Epoch>,
    /// Keep track of the last known timestamp for searching backward when timestamp is missing
    last_ts: LastTS,
    /// The target user config
    config: &'a TargetConfig,
}

#[derive(Copy, Clone)]
enum LastTS {
    Missing,
    KnownTS(Option<Epoch>, usize),
}

impl<IR: IndexReader, R: Read> Iterator for ChunkProcessor<'_, IR, R> {
    type Item = Result<AnomalyContext>;

    fn next(&mut self) -> Option<Self::Item> {
        self.anomalies
            .pop_front()
            .map(Ok)
            .or_else(|| match self.read_anomalies() {
                // When read_anomalies doesn't push new anomalies, that means we reach the end.
                Ok(()) if self.anomalies.is_empty() => None,
                Ok(()) => self.next(),
                Err(e) => Some(Err(e)),
            })
    }
}

impl<'a, IR: IndexReader, R: Read> ChunkProcessor<'a, IR, R> {
    pub fn new(
        reader: LinesIterator<R>,
        index: &'a IR,
        is_job_output: bool,
        skip_lines: (&'a mut Option<KnownLines>, Arc<Mutex<Option<KnownLines>>>),
        config: &'a TargetConfig,
        gl_date: Option<Epoch>,
    ) -> ChunkProcessor<'a, IR, R> {
        ChunkProcessor {
            reader,
            index,
            is_job_output,
            buffer: Vec::new(),
            left_overs: Vec::new(),
            targets: Vec::with_capacity(CHUNK_SIZE),
            targets_coord: Vec::with_capacity(CHUNK_SIZE),
            current_anomaly: None,
            anomalies: VecDeque::new(),
            skip_lines,
            coord: 0,
            line_count: 0,
            byte_count: 0,
            gl_date,
            last_ts: LastTS::KnownTS(None, 0),
            config,
        }
    }

    fn get_timestamp(&self, log_line: &str, buffer_pos: usize) -> Option<Epoch> {
        match self.last_ts {
            // This source does not contain timestamp, so don't bother trying to decode further lines
            LastTS::Missing => None,

            LastTS::KnownTS(_, last_ts_pos) => crate::timestamps::parse_timestamp(log_line)
                .or_else(|| self.get_closest_timestamp(0, buffer_pos, last_ts_pos))
                .and_then(|ts| match ts {
                    crate::timestamps::TS::Full(ts) => Some(ts),
                    crate::timestamps::TS::Time(time) => {
                        self.gl_date.map(|ts| crate::timestamps::set_date(ts, time))
                    }
                }),
        }
    }

    fn get_closest_timestamp(&self, count: usize, buffer_pos: usize, last_ts: usize) -> Option<TS> {
        if count > 32 {
            // We couldn't find a timestamp close enough
            None
        } else if let Some(prev_pos) = buffer_pos.checked_sub(1) {
            let ((bytes, line_number), _) = &self.buffer[prev_pos];
            if *line_number <= last_ts {
                // We reach the previously known timestamp
                None
            } else {
                let raw_str = logjuicer_iterator::clone_bytes_to_string(bytes).unwrap();
                crate::timestamps::parse_timestamp(&raw_str)
                    .or_else(|| self.get_closest_timestamp(count + 1, prev_pos, last_ts))
            }
        } else {
            // TODO: look in the left-overs
            None
        }
    }

    fn read_anomalies(&mut self) -> Result<()> {
        while let Some(line) = self.reader.next() {
            let line = line?;
            let raw_str = std::str::from_utf8(&line.0[..])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            self.line_count += 1;
            self.byte_count += line.0.len();
            self.coord += 1;

            // Special check to break when we are processing ourself
            if self.is_job_output && raw_str.contains("TASK [run-logjuicer") {
                break;
            }

            if self.config.is_ignored_line(raw_str) {
                continue;
            }

            // Call the static method of the ChunkIndex trait
            let tokens = logjuicer_tokenizer::process(raw_str);

            // Keep in the buffer all the lines until we get CHUNK_SIZE unique lines
            self.buffer.push((line, self.coord));

            let process_line = if let Some(skip_lines) = self.skip_lines.0 {
                skip_lines.insert(&tokens)
            } else {
                // TODO: this is not great because we are re-computing the same distance,
                // instead we should keep a record and re-use known value.
                // though, it's probably a lot of work...
                true
            };

            if process_line {
                self.targets.push(tokens);
                self.targets_coord.push(self.coord);

                if self.targets.len() == CHUNK_SIZE {
                    self.do_search_anomalies();
                    if !self.anomalies.is_empty() {
                        return Ok(());
                    }
                }
            } else if self.buffer.len() > CHUNK_SIZE * 10 {
                // the source contains mostly duplicate line.
                self.do_search_anomalies();
                if !self.anomalies.is_empty() {
                    return Ok(());
                }
            }
        }

        // We reached the end of the file and the last chunk is not completed
        if !self.targets.is_empty() {
            self.do_search_anomalies();
        }
        if let Some(anomaly) = &self.current_anomaly {
            // No more after context available
            self.anomalies.push_back(anomaly.clone());
            self.current_anomaly = None;
        }
        Ok(())
    }

    /// Helper function for the anomalies_from_reader implementation.
    fn do_search_anomalies(&mut self) {
        let distances = self.index.distance(&self.targets);

        let mut buffer_pos = 0;
        let mut last_context_pos = 0;

        for (target_idx, (distance, coord)) in
            distances.iter().zip(self.targets_coord.iter()).enumerate()
        {
            let is_anomaly = distance > &THRESHOLD;

            // The distances and coords are out of sync with the buffer, because they only contains unique line.
            // Thus for each distance, we need to find the matching raw lines in the buffer.
            let mut target_str = None;
            let buffer = &self.buffer[buffer_pos..];
            for ((bytes, line_number), line_coord) in buffer {
                buffer_pos += 1;
                let distance_found_in_buffer = line_coord == coord;

                if distance_found_in_buffer && is_anomaly {
                    // We found the target in the buffer, and it is an anomaly
                    let raw_str = logjuicer_iterator::clone_bytes_to_string(bytes).unwrap();
                    target_str = Some((&self.targets[target_idx], raw_str, line_number));
                } else if let Some(anomaly) = &mut self.current_anomaly {
                    // The buffer head is not anomaly, and we are still processing the last anomaly found.
                    // In that case, we add the log line to the after context.
                    let raw_str = logjuicer_iterator::clone_bytes_to_string(bytes).unwrap();
                    anomaly.after.push(raw_str);
                    if anomaly.after.len() >= CTX_LENGTH {
                        // The current anomaly is completed. TODO: try using std::mem::replace
                        self.anomalies.push_back(anomaly.clone());
                        self.current_anomaly = None;
                    }
                    // And we update the last context pos to adjust the next anomaly before context.
                    last_context_pos = buffer_pos;
                }
                if distance_found_in_buffer {
                    break;
                }
            }

            if let Some((log_tokens, log_line, log_pos)) = target_str {
                if let Some(anomaly) = &self.current_anomaly {
                    // We can push the current anomaly because any needed after context would overlap with the current anomaly.
                    self.anomalies.push_back(anomaly.clone());
                    self.current_anomaly = None;
                }

                if let Some(ref mut gl_skip_lines) = *self.skip_lines.1.lock().unwrap() {
                    if !gl_skip_lines.insert(log_tokens) {
                        continue;
                    }
                }

                // Parse timestamp from current line
                let timestamp = self.get_timestamp(&log_line, buffer_pos);
                self.last_ts = match (self.last_ts, timestamp) {
                    // It looks like this source has no timestamps
                    (LastTS::KnownTS(None, _), None) if *log_pos > 42 => LastTS::Missing,
                    (_, ts) => LastTS::KnownTS(ts, *log_pos),
                };

                // Grab before context
                let before = collect_before(
                    buffer_pos - 1,
                    last_context_pos,
                    &self.buffer,
                    &self.left_overs,
                );

                last_context_pos = buffer_pos;

                self.current_anomaly = Some(AnomalyContext {
                    before,
                    after: Vec::new(),
                    anomaly: Anomaly {
                        distance: *distance,
                        pos: *log_pos,
                        timestamp,
                        line: log_line,
                    },
                });
            } else if is_anomaly {
                panic!(
                    "Could not find target_coord {:?} in buffer {:#?} (starting at {})",
                    coord, self.buffer, buffer_pos
                );
            }
        }

        // Handle the last anomaly after context
        if let Some(anomaly) = &mut self.current_anomaly {
            if last_context_pos < self.buffer.len() {
                for ((bytes, _), _) in &self.buffer[last_context_pos..] {
                    let raw_str = logjuicer_iterator::clone_bytes_to_string(bytes).unwrap();
                    anomaly.after.push(raw_str);
                    if anomaly.after.len() >= CTX_LENGTH {
                        // The current anomaly is completed. TODO: try using std::mem::replace
                        self.anomalies.push_back(anomaly.clone());
                        self.current_anomaly = None;
                        break;
                    }
                }
            }
        }
        self.reset(last_context_pos)
    }

    fn reset(&mut self, left_overs_pos: usize) {
        self.targets.clear();
        self.targets_coord.clear();

        // Keep the buffer left over as potential prev context for the next anomaly.
        let min_left_overs_pos = if self.buffer.len() < BETWEEN_CTX_LENGTH {
            0
        } else {
            self.buffer.len() - BETWEEN_CTX_LENGTH
        };
        let max_left_overs_pos = left_overs_pos.max(min_left_overs_pos);
        self.left_overs = self.buffer[max_left_overs_pos..]
            .iter()
            // TODO: use direct bytes -> str conversion.
            .map(|((bytes, _), _)| logjuicer_iterator::clone_bytes_to_string(bytes).unwrap())
            .collect();
        self.buffer.clear();
    }
}

/// Build the before context from the buffer and the left_overs
///
/// * `buffer_pos` - the current position in the buffer.
/// * `last_context_pos` - the position of the last context (to be excluded).
fn collect_before(
    buffer_pos: usize,
    last_context_pos: usize,
    buffer: &[(LogLine, usize)],
    left_overs: &[Arc<str>],
) -> Vec<Arc<str>> {
    // extend the CTX_LENGTH when the last contex falls under the MAX_DISTANCE
    let ctx_distance = if buffer_pos - last_context_pos < BETWEEN_CTX_LENGTH {
        BETWEEN_CTX_LENGTH
    } else {
        CTX_LENGTH
    };
    let min_pos = buffer_pos.saturating_sub(ctx_distance);
    // The before context starts either at the last context pos, or the min pos.
    let before_context_pos = last_context_pos.max(min_pos);
    let mut before = buffer[before_context_pos..buffer_pos]
        .iter()
        // TODO: use direct bytes -> str conversion.
        .map(|((bytes, _), _)| logjuicer_iterator::clone_bytes_to_string(bytes).unwrap())
        .collect::<Vec<Arc<str>>>();
    if before_context_pos == 0 && before.len() < ctx_distance {
        // The anomaly happens at the begining of the buffer
        let need = ctx_distance - before.len();
        let available = left_overs.len();
        let want = need.min(available);
        let mut before_extra: Vec<Arc<str>> = left_overs[(available - want)..].to_vec();
        before.append(&mut before_extra);
        // Rotate the buffer to keep the left overs before
        before.rotate_right(want);
    }
    before
}

#[test]
fn test_leftovers() {
    let config = &TargetConfig::default();
    let index = logjuicer_index::index_mat(&[]);
    let mut skip_lines = Some(KnownLines::new());
    let reader = std::io::Cursor::new("");
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(reader, false));
    let skip_lines = (&mut skip_lines, Arc::new(Mutex::new(None)));
    let mut cp = ChunkProcessor::new(reader, &index, false, skip_lines, config, None);

    cp.buffer.push((("001 log line".into(), 0), 0));
    cp.buffer.push((("002 log line".into(), 1), 1));
    cp.buffer.push((("003 log line".into(), 2), 2));
    cp.buffer.push((("004 log line".into(), 3), 3));
    cp.buffer.push((("005 log line".into(), 4), 4));

    // Without left-overs
    assert_eq!(
        collect_before(0, 0, &cp.buffer, &cp.left_overs).len(),
        0,
        "We are at position 0, no before context available"
    );
    assert_eq!(
        collect_before(1, 0, &cp.buffer, &cp.left_overs),
        vec!["001 log line".into()],
        "We are at position 1, only 1 before is available"
    );
    assert_eq!(
        collect_before(1, 1, &cp.buffer, &cp.left_overs).len(),
        0,
        "If the last context is also at one, then no before context can be found"
    );
    assert_eq!(collect_before(2, 2, &cp.buffer, &cp.left_overs).len(), 0);
    assert_eq!(
        collect_before(4, 0, &cp.buffer, &cp.left_overs),
        vec![
            "001 log line".into(),
            "002 log line".into(),
            "003 log line".into(),
            "004 log line".into()
        ]
    );

    // With left-overs
    cp.reset(3);
    assert_eq!(cp.buffer.len(), 0, "After a reset, the buffer is empty");
    assert_eq!(
        cp.left_overs,
        vec!["004 log line".into(), "005 log line".into()],
        "The left over should contain unprocessed lines"
    );
    cp.buffer.push((("006 log line".into(), 6), 6));
    assert_eq!(
        collect_before(1, 0, &cp.buffer, &cp.left_overs),
        vec![
            "004 log line".into(),
            "005 log line".into(),
            "006 log line".into()
        ]
    );
}

#[test]
fn test_chunk_processor() {
    let config = &TargetConfig::default();
    let baseline = std::io::Cursor::new(["001: regular log line", "in-between line"].join("\n"));

    let mut trainer = IndexTrainer::new(logjuicer_index::FeaturesMatrixBuilder::default());
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(baseline, false));
    trainer.add(&TargetConfig::default(), reader).unwrap();
    let index = trainer.build();

    let data = std::io::Cursor::new(
        [
            "001: regular log line",
            "002: regular log line",
            "Traceback oops",
            "in-between line",
            "another Traceback",
            "003: regular log line",
        ]
        .join("\n"),
    );
    let mut anomalies = Vec::new();
    let mut skip_lines = Some(KnownLines::new());
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(data, false));
    let skip_lines = (&mut skip_lines, Arc::new(Mutex::new(None)));
    let processor = ChunkProcessor::new(reader, &index, false, skip_lines, config, None);
    for anomaly in processor {
        let anomaly = anomaly.unwrap();
        println!("anomalies: {:?}", anomaly);
        anomalies.push(anomaly);
        assert!(anomalies.len() <= 3)
    }
    let expected = vec![
        AnomalyContext {
            before: vec![
                "001: regular log line".into(),
                "002: regular log line".into(),
            ],
            after: vec!["in-between line".into()],
            anomaly: Anomaly {
                distance: 1.0,
                pos: 3,
                timestamp: None,
                line: "Traceback oops".into(),
            },
        },
        AnomalyContext {
            before: Vec::new(),
            after: vec!["003: regular log line".into()],
            anomaly: Anomaly {
                distance: 1.0,
                pos: 5,
                timestamp: None,
                line: "another Traceback".into(),
            },
        },
    ];
    assert_eq!(anomalies.len(), expected.len());
    anomalies
        .iter()
        .zip(expected.iter())
        .for_each(|(got, expected)| {
            assert_eq!(got.anomaly.line, expected.anomaly.line);
            assert_eq!(got.anomaly.pos, expected.anomaly.pos);
            assert!((got.anomaly.distance - expected.anomaly.distance).abs() < 0.001);
            assert_eq!(got.before, expected.before);
            assert_eq!(got.after, expected.after);
        });
}

#[test]
fn test_extended_context() {
    let config = &TargetConfig::default();
    let baseline = std::io::Cursor::new(
        [
            "001: regular log line",
            "in-between line",
            "extra context line",
        ]
        .join("\n"),
    );

    let mut trainer = IndexTrainer::new(logjuicer_index::FeaturesMatrixBuilder::default());
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(baseline, false));
    trainer.add(config, reader).unwrap();
    let index = trainer.build();

    let data = std::io::Cursor::new(
        [
            "001: regular log line",
            "Traceback oops",
            "in-between line",
            "in-between line",
            "in-between line",
            // The extra context shall be included because it falls in the BETWEEN_CTX_LENGTH
            "extra context line",
            "in-between line",
            "in-between line",
            "in-between line",
            "another Traceback",
            "003: regular log line",
        ]
        .join("\n"),
    );
    let mut anomalies = Vec::new();
    let mut skip_lines = Some(KnownLines::new());
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(data, false));
    let skip_lines = (&mut skip_lines, Arc::new(Mutex::new(None)));
    let processor = ChunkProcessor::new(reader, &index, false, skip_lines, config, None);
    for anomaly in processor {
        let anomaly = anomaly.unwrap();
        println!("anomalies: {:?}", anomaly);
        anomalies.push(anomaly);
        assert!(anomalies.len() <= 2)
    }
    let expected = vec![
        AnomalyContext {
            before: vec!["001: regular log line".into()],
            after: vec![
                "in-between line".into(),
                "in-between line".into(),
                "in-between line".into(),
            ],
            anomaly: Anomaly {
                distance: 1.0,
                pos: 2,
                timestamp: None,
                line: "Traceback oops".into(),
            },
        },
        AnomalyContext {
            before: vec![
                "extra context line".into(),
                "in-between line".into(),
                "in-between line".into(),
                "in-between line".into(),
            ],
            after: vec!["003: regular log line".into()],
            anomaly: Anomaly {
                distance: 1.0,
                pos: 10,
                timestamp: None,
                line: "another Traceback".into(),
            },
        },
    ];
    assert_eq!(anomalies.len(), expected.len());
    anomalies
        .iter()
        .zip(expected.iter())
        .for_each(|(got, expected)| {
            assert_eq!(got.anomaly.line, expected.anomaly.line);
            assert_eq!(got.anomaly.pos, expected.anomaly.pos);
            assert!((got.anomaly.distance - expected.anomaly.distance).abs() < 0.001);
            assert_eq!(got.before, expected.before);
            assert_eq!(got.after, expected.after);
        });
}

#[test]
fn test_process_config() {
    let cfg = crate::config::config_from_yaml(
        "
ignore_patterns:
  - fetch logs
  - get logs
",
    );
    let baseline = std::io::Cursor::new(
        [
            "001: regular log line",
            "in-between line",
            "extra context line",
        ]
        .join("\n"),
    );

    let config = cfg.get_target_config(&logjuicer_report::Content::sample("test"));

    let mut trainer = IndexTrainer::new(logjuicer_index::FeaturesMatrixBuilder::default());
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(baseline, false));
    trainer.add(config, reader).unwrap();
    let index = trainer.build();
    let data = std::io::Cursor::new(
        [
            "001: regular log line",
            "TASK fetch logs",
            "2024-03-19 get logs done",
            "Traceback oops",
        ]
        .join("\n"),
    );
    let mut skip_lines = Some(KnownLines::new());
    let reader = LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(data, false));
    let skip_lines = (&mut skip_lines, Arc::new(Mutex::new(None)));
    let processor = ChunkProcessor::new(reader, &index, false, skip_lines, config, None);
    let anomalies = processor.into_iter().collect::<Vec<_>>();
    assert_eq!(anomalies.len(), 1);
}
