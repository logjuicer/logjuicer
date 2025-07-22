// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use bytes::Bytes;
use std::io::Read;

use crate::{env::Env, files::file_open, reader::DecompressReader, urls::url_open};
use logjuicer_report::Source;

pub enum LinesIterator<R: Read> {
    Bytes(logjuicer_iterator::BytesLines<R>),
    Journal(logjuicer_journal::JournalLines),
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
        if source.as_str().ends_with(".journal") {
            match source {
                Source::Local(_, path_buf) => Ok(LinesIterator::Journal(
                    logjuicer_journal::JournalLines::new(path_buf)?,
                )),
                Source::Remote(_, _) => {
                    Err(anyhow::anyhow!("Remote journal file is not supported"))
                }
            }
        } else {
            let reader = match source {
                Source::Local(_, path_buf) => file_open(path_buf.as_path()),
                Source::Remote(_, url) => url_open(env, url),
            }?;
            Ok(LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(
                reader,
                source.is_json(),
            )))
        }
    }
}
