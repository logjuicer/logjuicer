// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use bytes::Bytes;
use std::io::Read;

use crate::{env::Env, journal::JournalLines, reader::DecompressReader};
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

impl<'a> LinesIterator<DecompressReader<'a>> {
    pub fn new(
        source: &Source,
        reader: DecompressReader<'a>,
    ) -> Result<LinesIterator<DecompressReader<'a>>> {
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

fn open_source(env: &Env, source: &Source) -> Result<crate::reader::DecompressReaderFile> {
    match source {
        Source::Local(_, path_buf) => crate::files::file_open(path_buf.as_path()),
        Source::Remote(_, url) => crate::urls::url_open(env, url),
        Source::TarFile(_, _, _) => Err(anyhow::anyhow!(
            "This is not possible, open_source doesn't work with TarFile.",
        )),
    }
}

pub fn open_single_source<'a>(
    env: &Env,
    source: &Source,
) -> Result<crate::reader::DecompressReader<'a>> {
    Ok(DecompressReader::Raw(open_source(env, source)?))
}

pub fn with_source<F>(env: &crate::env::Env, source: Source, mut cb: F)
where
    F: for<'a> FnMut(Source, std::result::Result<DecompressReader<'a>, String>),
{
    match open_source(env, &source) {
        Ok(reader) => {
            if source.is_tarball() {
                let reader = xz::read::XzDecoder::new(reader);
                let mut archive = tar::Archive::new(reader);
                match archive.entries() {
                    Ok(entries) => {
                        let source = std::sync::Arc::new(source);
                        for entry in entries {
                            match entry {
                                Ok(entry) => {
                                    let path = entry
                                        .path()
                                        .ok()
                                        .and_then(|p| p.as_os_str().to_str().map(|s| s.into()))
                                        .unwrap_or("unknown".into());
                                    let url = format!("{}?entry={}", source.as_str(), path);
                                    let new_source =
                                        Source::TarFile(Box::new(source.clone()), path, url.into());
                                    let reader = DecompressReader::TarballEntry(Box::new(entry));
                                    cb(new_source, Ok(reader))
                                }
                                Err(err) => cb(
                                    (*source).clone(),
                                    Err(format!("tarball entry failed: {}", err)),
                                ),
                            }
                        }
                    }
                    Err(err) => cb(source, Err(format!("tarball entries failed: {}", err))),
                }
            } else {
                cb(source, Ok(DecompressReader::Raw(reader)));
            }
        }
        Err(err) => cb(source, Err(format!("open_source failed: {}", err))),
    }
}
