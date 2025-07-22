// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use bytes::Bytes;
use std::io::Read;

use crate::{env::Env, files::file_open, reader::DecompressReader, urls::url_open};
use logjuicer_report::Source;

#[cfg(feature = "systemd-journal")]
pub enum LinesIterator<R: Read> {
    Bytes(logjuicer_iterator::BytesLines<R>),
    Journal(logjuicer_journal::JournalLines),
}

#[cfg(feature = "systemd-journal")]
impl<R: Read> Iterator for LinesIterator<R> {
    type Item = std::io::Result<(Bytes, usize)>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            LinesIterator::Bytes(it) => it.next(),
            LinesIterator::Journal(it) => it.next(),
        }
    }
}

#[cfg(not(feature = "systemd-journal"))]
pub struct LinesIterator<R: Read>(logjuicer_iterator::BytesLines<R>);

#[cfg(not(feature = "systemd-journal"))]
impl<R: Read> Iterator for LinesIterator<R> {
    type Item = std::io::Result<(Bytes, usize)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl LinesIterator<DecompressReader> {
    #[cfg(feature = "systemd-journal")]
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
            Ok(LinesIterator::Bytes(Self::new_bytes(env, source)?))
        }
    }

    #[cfg(not(feature = "systemd-journal"))]
    pub fn new(env: &Env, source: &Source) -> Result<LinesIterator<DecompressReader>> {
        Ok(LinesIterator(Self::new_bytes(env, source)?))
    }

    fn new_bytes(
        env: &Env,
        source: &Source,
    ) -> Result<logjuicer_iterator::BytesLines<DecompressReader>> {
        let reader = match source {
            Source::Local(_, path_buf) => file_open(path_buf.as_path()),
            Source::Remote(_, url) => url_open(env, url),
        }?;
        Ok(logjuicer_iterator::BytesLines::new(
            reader,
            source.is_json(),
        ))
    }
}
