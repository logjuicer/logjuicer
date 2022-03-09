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

pub struct BytesLines<R: Read> {
    reader: R,
    buf: BytesMut,
    state: State,
    line_count: usize,
}

// TODO: make this configurable.
const BUF_SIZE: usize = 4096;
const MAX_LINE_LEN: usize = 200;

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
        BytesLines {
            reader,
            state: State::Scanning(Sep::NewLine),
            buf: BytesMut::with_capacity(BUF_SIZE),
            line_count: 0,
        }
    }

    // Read a new chunk and call get_slice
    fn read_slice(&mut self) -> Option<Result<LogLine>> {
        let pos = self.buf.len();
        self.buf.resize(pos + BUF_SIZE, 0);
        match self.reader.read(&mut self.buf[pos..]) {
            // We read some data.
            Ok(n) if n > 0 => {
                if n < BUF_SIZE {
                    self.buf.truncate(pos + n);
                }
                self.get_slice()
            }

            // We reached the end of the reader, and there are left-overs.
            Ok(_) if pos > 0 => {
                if self.state == State::Scanning(Sep::NewLine) {
                    self.line_count += 1
                }
                self.state = State::EoF;
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
        let next_line_pos = self.find_next_line();
        // Here we check what is the current line minimum length
        let (min_line_length, sep) = match next_line_pos {
            // We found the end of the line at pos.
            Some((pos, t)) => (pos, Some(t)),
            // We haven't found the end of the line, so the len is at least the buffer size.
            None => (self.buf.len(), None),
        };

        match (min_line_length > MAX_LINE_LEN, next_line_pos) {
            // The current line is over the limit, we need to discard it.
            (true, _) if min_line_length < self.buf.len() => {
                // The next line already in the buffer, so we can just advance.
                self.buf.advance(min_line_length + sep.unwrap().len());
                self.get_slice()
            }

            // The current line is over the limit, and we don't know where it ends.
            (true, _) => {
                self.buf.clear();
                self.buf.reserve(BUF_SIZE);
                self.drain_line()
            }

            // We haven't found the end of the line, we need more data.
            (_, None) => {
                // reserve() will attempt to reclaim space in the existing buffer.
                self.buf.reserve(BUF_SIZE);
                self.read_slice()
            }

            // We found the end of the line, we can return it now.
            (_, Some((pos, t))) => {
                // This create a new zero copy reference to the existing before.
                let res = self.buf.split_to(pos).freeze();
                self.buf.advance(t.len());
                Some(Ok((res, self.line_count)))
            }
        }
    }

    // Find the next line position and update the line count
    fn find_next_line(&mut self) -> Option<(usize, Sep)> {
        let mut iter = self.buf.clone().into_iter().enumerate().peekable();
        // Here we scan each byte in a single pass.
        while let Some((pos, c)) = iter.next() {
            let sep = match c as char {
                // A new line separator is at pos
                '\n' => Some(Sep::NewLine),

                // A '\\' is at pos, let's peek the next char to see if it's a 'n'
                '\\' => iter.peek().and_then(|(_, c)| {
                    if *c as char == 'n' {
                        Some(Sep::SubLine)
                    } else {
                        None
                    }
                }),

                // Current character is not a separator
                _ => None,
            };
            if let Some(sep) = sep {
                // We found a separator.
                if self.state == State::Scanning(Sep::NewLine) {
                    self.line_count += 1
                }
                self.state = State::Scanning(sep);
                return Some((pos, sep));
            }
        }
        None
    }

    // Drop until we find the next line
    fn drain_line(&mut self) -> Option<Result<LogLine>> {
        self.buf.resize(BUF_SIZE, 0);
        match self.reader.read(&mut self.buf) {
            // We read some data.
            Ok(n) if n > 0 => match self.find_next_line() {
                Some((n, t)) if n < BUF_SIZE => {
                    // the next line is already in the buffer
                    self.buf.advance(n + t.len());
                    self.get_slice()
                }
                Some(_) => {
                    // the line terminated at the end of the buffer.
                    self.buf.clear();
                    self.read_slice()
                }
                None => {
                    // No line terminator found, keep on draining.
                    self.buf.clear();
                    self.drain_line()
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
        let lines: Result<Vec<LogLine>> = BytesLines::new(reader).collect();
        lines.unwrap()
    };
    let lines = get_lines(std::io::Cursor::new("first\nsecond\nthird\nfourth\\nsub4"));
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
}
