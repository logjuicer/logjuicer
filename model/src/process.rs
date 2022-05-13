// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides the core utilities to use logreduce-index with Read objects.

use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use std::io::Read;

use crate::{Anomaly, AnomalyContext, ChunkIndex};
use logreduce_iterator::LogLine;

const THRESHOLD: logreduce_index::F = 0.3;
const CTX_DISTANCE: usize = 3;
const CHUNK_SIZE: usize = 512;

/// Helper struct to manage indexing multiples readers.
pub struct ChunkTrainer<'a> {
    index: &'a mut ChunkIndex,
    skip_lines: HashSet<String>,
    baselines: Vec<String>,
    pub line_count: usize,
    pub byte_count: usize,
}

impl<'a> ChunkTrainer<'a> {
    pub fn new(index: &'a mut ChunkIndex) -> ChunkTrainer<'a> {
        ChunkTrainer {
            index,
            skip_lines: HashSet::new(),
            baselines: Vec::new(),
            line_count: 0,
            byte_count: 0,
        }
    }

    /// Index a single reader
    pub fn single<R: Read>(index: &'a mut ChunkIndex, read: R) -> Result<()> {
        let mut trainer = ChunkTrainer::new(index);
        trainer.add(read)?;
        trainer.complete();
        Ok(())
    }

    pub fn add<R: Read>(&mut self, read: R) -> Result<()> {
        for line in logreduce_iterator::BytesLines::new(read) {
            let line = line?;
            let raw_str = std::str::from_utf8(&line.0[..])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            self.line_count += 1;
            self.byte_count += line.0.len();
            let tokens = self.index.tokenize(raw_str);

            if !self.skip_lines.contains(&tokens) {
                self.skip_lines.insert(tokens.clone());
                self.baselines.push(tokens);

                if self.baselines.len() == CHUNK_SIZE {
                    self.index.add(&self.baselines);
                    self.baselines.clear();
                }
            }
        }
        Ok(())
    }

    pub fn complete(&mut self) {
        if !self.baselines.is_empty() {
            self.index.add(&self.baselines);
        }
    }
}

/// Helper struct to manage the log lines and the unique tokenized lines.
/// The goal is to perform the index search on unique lines, while keeping a
/// buffer of the raw line to manage the surrounding context.
pub struct ChunkProcessor<'a, R: Read> {
    reader: logreduce_iterator::BytesLines<R>,
    index: &'a ChunkIndex,
    /// The raw log line with their global position
    buffer: Vec<(logreduce_iterator::LogLine, usize)>,
    /// The target tokenized lines
    targets: Vec<String>,
    /// The target positions
    targets_coord: Vec<usize>,
    /// The very last lines of the current buffer that could be the prev context of the next chunk
    left_overs: Vec<String>,
    /// The current anomaly being processed
    current_anomaly: Option<AnomalyContext>,
    /// The list of anomalies recently found.
    anomalies: VecDeque<AnomalyContext>,
    /// The list of unique log lines, to avoid searching a line twice.
    skip_lines: &'a mut HashSet<String>,
    /// The current line coordinate.
    coord: usize,
    /// Total lines count
    pub line_count: usize,
    /// Total bytes count
    pub byte_count: usize,
}

impl<'a, R: Read> Iterator for ChunkProcessor<'a, R> {
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

impl<'a, R: Read> ChunkProcessor<'a, R> {
    pub fn new(
        read: R,
        index: &'a ChunkIndex,
        skip_lines: &'a mut HashSet<String>,
    ) -> ChunkProcessor<'a, R> {
        ChunkProcessor {
            reader: logreduce_iterator::BytesLines::new(read),
            index,
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
            if raw_str.contains("TASK [log-classify") {
                break;
            }

            // Call the static method of the ChunkIndex trait
            let tokens = self.index.tokenize(raw_str);

            // Keep in the buffer all the lines until we get CHUNK_SIZE unique lines
            self.buffer.push((line, self.coord));

            if !self.skip_lines.contains(&tokens) {
                self.skip_lines.insert(tokens.clone());

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
        let distances = self.index.search(&self.targets);

        let mut buffer_pos = 0;
        let mut last_context_pos = 0;

        for (distance, coord) in distances.iter().zip(self.targets_coord.iter()) {
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
                    let raw_str = logreduce_iterator::clone_bytes_to_string(bytes).unwrap();
                    target_str = Some((raw_str, line_number));
                } else if let Some(anomaly) = &mut self.current_anomaly {
                    // The buffer head is not anomaly, and we are still processing the last anomaly found.
                    // In that case, we add the log line to the after context.
                    let raw_str = logreduce_iterator::clone_bytes_to_string(bytes).unwrap();
                    anomaly.after.push(raw_str);
                    if anomaly.after.len() >= CTX_DISTANCE as usize {
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

            if let Some((log_line, log_pos)) = target_str {
                if let Some(anomaly) = &self.current_anomaly {
                    // We can push the current anomaly because any needed after context would overlap with the current anomaly.
                    self.anomalies.push_back(anomaly.clone());
                    self.current_anomaly = None;
                }

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
                    let raw_str = logreduce_iterator::clone_bytes_to_string(bytes).unwrap();
                    anomaly.after.push(raw_str);
                    if anomaly.after.len() >= CTX_DISTANCE as usize {
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
        let min_left_overs_pos = if self.buffer.len() < CTX_DISTANCE {
            0
        } else {
            self.buffer.len() - CTX_DISTANCE
        };
        let max_left_overs_pos = left_overs_pos.max(min_left_overs_pos);
        self.left_overs = self.buffer[max_left_overs_pos..]
            .iter()
            // TODO: use direct bytes -> str conversion.
            .map(|((bytes, _), _)| logreduce_iterator::clone_bytes_to_string(bytes).unwrap())
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
    left_overs: &[String],
) -> Vec<String> {
    let min_pos = if buffer_pos < CTX_DISTANCE {
        0
    } else {
        buffer_pos - CTX_DISTANCE
    };
    // The before context starts either at the last context pos, or the min pos.
    let before_context_pos = last_context_pos.max(min_pos);
    let mut before = buffer[before_context_pos..buffer_pos]
        .iter()
        // TODO: use direct bytes -> str conversion.
        .map(|((bytes, _), _)| logreduce_iterator::clone_bytes_to_string(bytes).unwrap())
        .collect::<Vec<String>>();
    if before_context_pos == 0 && before.len() < CTX_DISTANCE {
        // The anomaly happens at the begining of the buffer
        let need = CTX_DISTANCE as usize - before.len();
        let available = left_overs.len();
        let want = need.min(available);
        let mut before_extra: Vec<String> = left_overs[(available - want)..]
            .iter()
            .map(|s| s.to_string())
            .collect();
        before.append(&mut before_extra);
        // Rotate the buffer to keep the left overs before
        before.rotate_right(want);
    }
    before
}

#[test]
fn test_leftovers() {
    let index = crate::hashing_index::new();
    let mut skip_lines = HashSet::new();
    let mut cp = ChunkProcessor::new(std::io::Cursor::new(""), &index, &mut skip_lines);

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
        vec!["001 log line".to_string()],
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
            "002 log line".to_string(),
            "003 log line".to_string(),
            "004 log line".to_string()
        ]
    );

    // With left-overs
    cp.reset(3);
    assert_eq!(cp.buffer.len(), 0, "After a reset, the buffer is empty");
    assert_eq!(
        cp.left_overs,
        vec!["004 log line".to_string(), "005 log line".to_string()],
        "The left over should contain unprocessed lines"
    );
    cp.buffer.push((("006 log line".into(), 6), 6));
    assert_eq!(
        collect_before(1, 0, &cp.buffer, &cp.left_overs),
        vec![
            "004 log line".to_string(),
            "005 log line".to_string(),
            "006 log line".to_string()
        ]
    );
}

#[test]
fn test_chunk_processor() {
    let mut index = crate::hashing_index::new();
    let baseline = std::io::Cursor::new(["001: regular log line", "in-between line"].join("\n"));

    let mut trainer = ChunkTrainer::new(&mut index);
    trainer.add(baseline).unwrap();
    trainer.complete();

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
    let mut skip_lines = HashSet::new();
    let processor = ChunkProcessor::new(data, &index, &mut skip_lines);
    for anomaly in processor {
        let anomaly = anomaly.unwrap();
        anomalies.push(anomaly);
        if anomalies.len() > 3 {
            assert!(false);
        }
    }
    let expected = vec![
        AnomalyContext {
            before: vec![
                "001: regular log line".to_string(),
                "002: regular log line".to_string(),
            ],
            after: vec!["in-between line".to_string()],
            anomaly: Anomaly {
                distance: 1.0,
                pos: 3,
                line: "Traceback oops".to_string(),
            },
        },
        AnomalyContext {
            before: Vec::new(),
            after: vec!["003: regular log line".to_string()],
            anomaly: Anomaly {
                distance: 1.0,
                pos: 5,
                line: "another Traceback".to_string(),
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
