// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a BytesLines iterator for the [logreduce](https://github.com/logreduce/logreduce) project.
//!
//! The goals of this iterator are:
//!
//! - Split sub line to handle cmd output embedded as a long oneliner.
//! - Work with Read object, such as file decompressors or network endpoints.
//! - Constant memory usage by using zero copy [Bytes] slices.
//! - Line length limit to prevent overflow on invalid data.
//!
//! Here is an example usage:
//!
//! ```rust
//! use logreduce_iterator::BytesLines;
//! // Create a test in-memory reader.
//! let reader = std::io::Cursor::new("first\nsecond\\nextra");
//!
//! // Creates the iterator and unwrap error for assert_eq!.
//! let mut lines_iter = BytesLines::new(reader).map(|l| l.unwrap());
//! assert_eq!(lines_iter.next(), Some(("first".into(), 1)));
//! assert_eq!(lines_iter.next(), Some(("second".into(), 2)));
//! assert_eq!(lines_iter.next(), Some(("extra".into(), 2)));
//! assert_eq!(lines_iter.next(), None);
//! ```
//!
//! You can zero-copy convert a [Bytes] to [&str] using: `std::str::from_utf8(&bytes[..])`.

use bytes::{Buf, Bytes, BytesMut};
use std::io::{Read, Result};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Sep {
    // A line return: '\n'
    NewLine,
    // A litteral line return: '\\n'
    SubLine,
}

impl Sep {
    // The size of the separator
    fn len(&self) -> usize {
        match self {
            Sep::NewLine => 1,
            Sep::SubLine => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    // We reached the end of the file.
    EoF,
    // We are processing a line, keeping track of the last separator to properly increase the line count.
    Scanning(Sep),
}

/// The BytesLines struct holds a single buffer to store the read data and it yields immutable memory slice.
///
// Here is the main sequence diagram:
//
//     ⭩- the buffer starts here.
// A: [                          ]          < the buffer is empty, we read a chunk.
// B: [aaaaaaaaaaaa\nbbbbb\nccccc]          < there is a line separator.
// C:  ╰-----------⮡ next slice
// D:               ⭨
// B: [              bbbbb\nccccc]
// C:                ╰----⮡ next slice
// D:                      ⭨
// E: [                     ccccc]          < the line is incomplete.
// F:       ⭩ we reserve more space and move the left-overs at the begining of the buffer.
// G: [ccccc                           ]    < we read another chunk after the left-overs.
// B: [ccccccc\ndddddddddddddd\neeeeeee]
// C:  ╰------⮡ next slice
// D:          ⭨
// B: [         dddddddddddddd\neeeeeee]
// C:           ╰-------------⮡ next slice
// D:                          ⭨
// E: [                         eeeeeee]    < the line is incomplete.
// F:         ⭩ we reserve more space and move the left-overs at the begining of the buffer.
// G: [eeeeeee                           ]  < we read another chunk after the left-overs.
// H: [eeeeeeeee\n                       ]  < we reach the end of file.
// H   ╰--------⮡ the last slice
//
//
// There are two situations to handle lines that are over the length limits:
//
// I: [XXXXXXXXXXXXXXXXXXXX\nbbbb]          < the next line if in the buffer.
// I:                        ⭩- the buffer position advance
// I: [                      bbbb]          < we resume the iterator.
//
// J: [XXXXXXXXXXXXXXXXXXXXXXXXXX]          < the next line is not in the buffer.
// J: [                          ]          < we clear the buffer and repeat until we reach Step I.
pub struct BytesLines<R: Read> {
    reader: R,
    buf: BytesMut,
    state: State,
    line_count: usize,
    chunk_size: usize,
    max_line_length: usize,
}

/// Logline is a tuple (content, line number).
pub type LogLine = (Bytes, usize);

impl<R: Read> Iterator for BytesLines<R> {
    type Item = Result<LogLine>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            State::EoF => None,
            State::Scanning(_) if self.buf.is_empty() => self.read_slice(),
            State::Scanning(_) => self.get_slice(),
        }
    }
}

impl<R: Read> BytesLines<R> {
    /// Creates a new BytesLines.
    pub fn new(reader: R) -> BytesLines<R> {
        // TODO: make these configurable
        let chunk_size = 8192;
        let max_line_length = 6000;
        BytesLines {
            reader,
            max_line_length,
            chunk_size,
            state: State::Scanning(Sep::NewLine),
            buf: BytesMut::with_capacity(chunk_size),
            line_count: 0,
        }
    }

    // Read a new chunk and call get_slice
    fn read_slice(&mut self) -> Option<Result<LogLine>> {
        let pos = self.buf.len();
        // When pos is at 0, we are at Step A, otherwise this is Step G
        self.buf.resize(pos + self.chunk_size, 0);
        match self.reader.read(&mut self.buf[pos..]) {
            // We read some data.
            Ok(n) if n > 0 => {
                self.buf.truncate(pos + n);
                self.get_slice()
            }

            // Step H: We reached the end of the reader, but we have left-overs.
            Ok(_) if pos > 0 => {
                self.update_line_counter(State::EoF);
                Some(Ok((self.buf.split_to(pos).freeze(), self.line_count)))
            }

            // We reached the end of the reader, this is the end.
            Ok(_) => None,

            // There was a reading error, we return it.
            Err(e) => Some(Err(e)),
        }
    }

    // Find the next line in the buffer
    fn get_slice(&mut self) -> Option<Result<LogLine>> {
        match self.find_next_line() {
            // Step J: The current line is over the limit, and we don't know where it ends.
            None if self.buf.len() > self.max_line_length => {
                self.buf.clear();
                self.buf.reserve(self.chunk_size);
                self.drop_until_next_line()
            }

            // Step I: The current line is over the limit, we need to discard it.
            Some((pos, sep)) if pos > self.max_line_length => {
                // The next line is already in the buffer, so we can just advance.
                self.buf.advance(pos + sep.len());
                self.get_slice()
            }

            // Step E: We haven't found the end of the line, we need more data.
            None => {
                // Step F: reserve() will attempt to reclaim space in the buffer.
                self.buf.reserve(self.chunk_size);
                self.read_slice()
            }

            // Step B: We found the end of the line, we can return it now.
            Some((pos, sep)) => {
                // Step C: split_to() creates a new zero copy reference to the buffer.
                let res = self.buf.split_to(pos).freeze();
                // Step D: advance the starting position
                self.buf.advance(sep.len());
                Some(Ok((res, self.line_count)))
            }
        }
    }

    // Find the next line position and update the line count
    fn find_next_line(&mut self) -> Option<(usize, Sep)> {
        let slice = self.buf.as_ref();
        let size = slice.len();
        let char_is = |pos: usize, c: char| pos < size && slice[pos] == (c as u8);
        for (pos, c) in slice.iter().enumerate().take(size) {
            let c = *c as char;
            let sep = match c {
                '\n' => Some(Sep::NewLine),
                '\\' if char_is(pos + 1, 'n') => Some(Sep::SubLine),
                _ => None,
            };
            if let Some(sep) = sep {
                // We found a separator.
                self.update_line_counter(State::Scanning(sep));
                return Some((pos, sep));
            }
        }
        None
    }

    fn update_line_counter(&mut self, state: State) {
        // We only increase the line counter when the last separator was a new line.
        if self.state == State::Scanning(Sep::NewLine) {
            self.line_count += 1
        }
        self.state = state;
    }

    // Drop until we find the next line
    fn drop_until_next_line(&mut self) -> Option<Result<LogLine>> {
        self.buf.resize(self.chunk_size, 0);
        match self.reader.read(&mut self.buf) {
            // We read some data.
            Ok(n) if n > 0 => match self.find_next_line() {
                // the long line terminated at the end of the buffer.
                Some(_) if n == self.chunk_size => {
                    self.buf.clear();
                    self.read_slice()
                }

                // the next line is already in the buffer
                Some((pos, sep)) => {
                    self.buf.advance(pos + sep.len());
                    self.get_slice()
                }

                // No line terminator found, keep on draining.
                None => {
                    self.buf.clear();
                    self.drop_until_next_line()
                }
            },

            // We reached the end of the reader, this is the end.
            Ok(_) => None,

            // There was a reading error, we return it.
            Err(e) => Some(Err(e)),
        }
    }
}

#[test]
fn test_iterator() {
    let get_lines = |reader| -> Vec<LogLine> {
        let lines: Result<Vec<LogLine>> = BytesLines::new(std::io::Cursor::new(reader)).collect();
        lines.unwrap()
    };

    let lines = get_lines("first\nsecond\nthird\nfourth\\nsub4");
    assert_eq!(
        lines,
        vec![
            ("first".into(), 1),
            ("second".into(), 2),
            ("third".into(), 3),
            ("fourth".into(), 4),
            ("sub4".into(), 4),
        ]
    );

    let lines = get_lines("first\\n");
    assert_eq!(lines, vec![("first".into(), 1)]);
}
