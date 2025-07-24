// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use bytes::Bytes;
use std::io::Read;

use crate::{
    env::Env, files::file_open, journal::JournalLines, reader::DecompressReader, urls::url_open,
};
use logjuicer_report::Source;

pub enum LinesIterator<R: Read> {
    Bytes(logjuicer_iterator::BytesLines<R>),
    Journal(JournalLines<R>),
}

impl<R: Read> Iterator for LinesIterator<R> {
    type Item = std::io::Result<(Bytes, usize)>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LinesIterator::Bytes(it) => it.next(),
            LinesIterator::Journal(it) => it.next(),
        }
    }
}

impl LinesIterator<DecompressReader> {
    pub fn new(env: &Env, source: &Source) -> Result<LinesIterator<DecompressReader>> {
        let reader = match source {
            Source::Local(_, path_buf) => file_open(path_buf.as_path()),
            Source::Remote(_, url) => url_open(env, url),
        }?;
        let iter = if source.as_str().ends_with(".journal") {
            LinesIterator::Journal(JournalLines::new(reader)?)
        } else {
            LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(
                reader,
                source.is_json(),
            ))
        };
        Ok(iter)
    }
}
