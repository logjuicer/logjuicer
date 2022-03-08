// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::io::{Read, Result};

use bytes::{Buf, Bytes, BytesMut};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Sep {
    NewLine,
    SubLine,
}

impl Sep {
    fn len(&self) -> usize {
        match self {
            Sep::NewLine => 1,
            Sep::SubLine => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    EOF,
    Running(Sep),
}

pub struct BytesLines<R: Read> {
    reader: R,
    buf: BytesMut,
    state: State,
    pos: usize,
}

const BUF_SIZE: usize = 4096;
const MAX_LINE_LEN: usize = 200;

/// Logline is a tuple (content, line number)
pub type LogLine = (Bytes, usize);

impl<R: Read> Iterator for BytesLines<R> {
    type Item = Result<LogLine>;

    fn next(&mut self) -> Option<Self::Item> {
        // println!("next() self.buf {:?} state {:?}", self.buf, self.state);
        match self.state {
            State::EOF => None,
            _ if self.buf.is_empty() => self.read_slice(0),
            _ => self.get_slice(),
        }
    }
}

impl<R: Read> BytesLines<R> {
    pub fn new(reader: R) -> BytesLines<R> {
        BytesLines {
            reader,
            state: State::Running(Sep::NewLine),
            buf: BytesMut::with_capacity(BUF_SIZE),
            pos: 0,
        }
    }

    // Fill up the buffer at position and call get_slice
    fn read_slice(&mut self, pos: usize) -> Option<Result<LogLine>> {
        self.buf.resize(pos + BUF_SIZE, 0);
        match self.reader.read(&mut self.buf[pos..]) {
            Ok(n) if n > 0 => {
                // println!("Read {} bytes", n);
                if n < BUF_SIZE {
                    self.buf.truncate(pos + n);
                }
                self.get_slice()
            }
            Ok(_) => {
                // println!("Empty read, current buf {} at pos {}", self.buf.len(), pos);
                self.increase_pos(Sep::NewLine);
                self.state = State::EOF;
                self.buf.truncate(pos);
                if self.buf.is_empty() {
                    None
                } else {
                    Some(Ok((self.buf.split_to(pos).freeze(), self.pos)))
                }
            }
            Err(e) => Some(Err(e)),
        }
    }

    fn get_slice(&mut self) -> Option<Result<LogLine>> {
        let next_line_pos = self.find_next_line();
        let (min_line_length, sep) = match next_line_pos {
            Some((pos, t)) => (pos, Some(t)),
            None => (self.buf.len(), None),
        };
        match (min_line_length > MAX_LINE_LEN, next_line_pos) {
            (true, _) => {
                // println!("line is too long! {} {}", min_line_length, self.buf.len(),);
                if min_line_length < self.buf.len() {
                    self.buf
                        .advance(min_line_length + sep.map(|sep| sep.len()).unwrap_or(0));
                    self.get_slice()
                } else {
                    self.buf.clear();
                    self.buf.reserve(BUF_SIZE);
                    self.drain_line()
                }
            }
            (_, None) => {
                // println!("No end of line found, need to read more");
                //
                // reserve() moves the left-over data at the begining of the buffer.
                self.buf.reserve(BUF_SIZE);
                self.read_slice(self.buf.len())
            }
            (_, Some((pos, t))) => {
                // println!("Found line return at {}", pos);
                self.make_slice(pos, t)
            }
        }
    }

    fn increase_pos(&mut self, sep: Sep) {
        self.pos += match (sep, self.state) {
            (Sep::NewLine, State::Running(Sep::NewLine)) => 1,
            (Sep::SubLine, State::Running(Sep::NewLine)) => 1,
            _ => 0,
        };
    }

    // Find the next line position and update the line count
    fn find_next_line(&mut self) -> Option<(usize, Sep)> {
        let mut iter = self.buf.clone().into_iter().enumerate().peekable();
        while let Some((pos, c)) = iter.next() {
            let sep = match c as char {
                '\n' => Some(Sep::NewLine),
                '\\' => iter.peek().and_then(|(_, c)| {
                    if *c == ('n' as u8) {
                        Some(Sep::SubLine)
                    } else {
                        None
                    }
                }),
                _ => None,
            };
            match sep {
                Some(sep) => {
                    self.increase_pos(sep);
                    self.state = State::Running(sep);
                    return Some((pos, sep));
                }
                None => continue,
            }
        }
        None
    }

    fn make_slice(&mut self, pos: usize, t: Sep) -> Option<Result<LogLine>> {
        // println!("Making a slice {}", pos);
        let res = self.buf.split_to(pos).freeze();
        self.buf.advance(t.len());
        Some(Ok((res, self.pos)))
    }

    // Drop while we find the next line
    fn drain_line(&mut self) -> Option<Result<LogLine>> {
        // println!("Draining")
        self.buf.resize(BUF_SIZE, 0);
        match self.reader.read(&mut self.buf) {
            Ok(n) if n > 0 => match self.find_next_line() {
                Some((n, t)) if n < BUF_SIZE => {
                    // the next line is already in the buffer
                    self.buf.advance(n + t.len());
                    self.get_slice()
                }
                Some(_) => {
                    // the line terminated at the end of the buffer.
                    self.buf.clear();
                    self.read_slice(0)
                }
                None => self.drain_line(),
            },
            Ok(_) => {
                self.state = State::EOF;
                None
            }
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
