// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use bytes::Bytes;
use std::io::Read;

use crate::{env::Env, journal::JournalLines, reader::DecompressReader};
use logjuicer_report::{Source, SourceLoc};

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

fn open_source(env: &Env, source: &SourceLoc) -> Result<crate::reader::DecompressReaderFile> {
    match source {
        SourceLoc::Local(_, path_buf) => crate::files::file_open(path_buf.as_path()),
        SourceLoc::Remote(_, url) => crate::urls::url_open(env, url),
    }
}

pub fn open_single_source<'a>(
    env: &Env,
    source: &Source,
) -> Result<crate::reader::DecompressReader<'a>> {
    match source {
        Source::RawFile(source) => Ok(DecompressReader::Raw(open_source(env, source)?)),
        Source::TarFile(_, _, _) => Err(anyhow::anyhow!(
            "This is not possible, open_source doesn't work with TarFile.",
        )),
    }
}

pub fn with_source<F>(env: &crate::env::TargetEnv<'_>, source: SourceLoc, mut cb: F)
where
    F: for<'a> FnMut(Source, std::result::Result<DecompressReader<'a>, String>),
{
    match open_source(env.gl, &source) {
        Ok(reader) => {
            if source.is_tarball() {
                let reader = liblzma::read::XzDecoder::new(reader);
                let mut archive = tar::Archive::new(reader);
                match archive.entries() {
                    Ok(entries) => {
                        let source = std::sync::Arc::new(source);
                        for entry in entries {
                            match entry {
                                Ok(entry) => {
                                    if !entry.header().entry_type().is_file() {
                                        continue;
                                    }
                                    let path: std::sync::Arc<str> = entry
                                        .path()
                                        .ok()
                                        .and_then(|p| p.as_os_str().to_str().map(|s| s.into()))
                                        .unwrap_or("unknown".into());
                                    if !env.config.is_fp_valid(&path) {
                                        continue;
                                    }
                                    let url = format!("{}?entry={}", source.as_str(), path);
                                    let reader = if path.ends_with(".gz") {
                                        DecompressReader::TarballEntryCompressed(Box::new(
                                            flate2::read::GzDecoder::new(entry),
                                        ))
                                    } else {
                                        DecompressReader::TarballEntry(Box::new(entry))
                                    };
                                    let new_source =
                                        Source::TarFile(source.clone(), path, url.into());
                                    cb(new_source, Ok(reader))
                                }
                                Err(err) => cb(
                                    Source::RawFile((*source).clone()),
                                    Err(format!("tarball entry failed: {}", err)),
                                ),
                            }
                        }
                    }
                    Err(err) => cb(
                        Source::RawFile(source),
                        Err(format!("tarball entries failed: {}", err)),
                    ),
                }
            } else {
                cb(Source::RawFile(source), Ok(DecompressReader::Raw(reader)));
            }
        }
        Err(err) => cb(
            Source::RawFile(source),
            Err(format!("open_source failed: {}", err)),
        ),
    }
}
