// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a transparent decompression reader.

use anyhow::Result;
use std::io::Read;
use std::path::Path;
use url::Url;

use std::fs::File;

use crate::env::Env;
use flate2::read::GzDecoder;

fn is_success(code: ureq::http::StatusCode) -> bool {
    (200..400).contains(&code.as_u16())
}

// allow large enum for gzdecoder, which are the most used
#[allow(clippy::large_enum_variant)]
pub enum DecompressReaderFile {
    Flat(File),
    Gz(GzDecoder<File>),
    // TODO: support BZIP2 compression
    Remote(UreqReader),
}
use DecompressReaderFile::*;

pub enum DecompressReader<'a> {
    Raw(DecompressReaderFile),
    Nested(Box<dyn Read + 'a>),
}
use DecompressReader::*;

type UreqReader = Box<dyn Read + Send + Sync + 'static>;

pub fn from_path(path: &Path) -> Result<DecompressReaderFile> {
    let fp = File::open(path)?;
    let extension = path.extension().unwrap_or_else(|| std::ffi::OsStr::new(""));
    Ok(if extension == ".gz" {
        Gz(GzDecoder::new(fp))
    } else {
        Flat(fp)
    })
}

fn with_auth<A>(env: &Env, req: ureq::RequestBuilder<A>) -> ureq::RequestBuilder<A> {
    match &env.auth {
        None => req,
        Some((k, v)) => req.header(k.as_ref(), v.as_ref()),
    }
}

pub fn head_url(env: &Env, url: &Url) -> Result<bool> {
    let resp = with_auth(env, env.client.head(url.as_str())).call()?;
    Ok(is_success(resp.status()))
}

pub fn get_url(env: &Env, url: &Url) -> Result<DecompressReaderFile> {
    let resp = with_auth(env, env.client.get(url.as_str())).call()?;
    Ok(Remote(Box::new(resp.into_body().into_reader())))
}

impl Read for DecompressReaderFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // TODO: refactor using the enum_dispatch crate.
        match self {
            Flat(r) => r.read(buf),
            Gz(r) => r.read(buf),
            Remote(r) => r.read(buf),
        }
    }
}

impl Read for DecompressReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Raw(r) => r.read(buf),
            Nested(r) => r.read(buf),
        }
    }
}

/*
// Automatic decompressor implementation poc
pub fn auto<R: Read + 'static>(mut reader: R) -> Result<Box<dyn Read>> {
    // peak at the data
    let mut buf = Vec::new();
    buf.resize(4096, 0);
    let read_count = reader.read(&mut buf)?;
    buf.resize(read_count, 0);
    // recreate a reader
    let new_reader = std::io::Cursor::new(buf).chain(reader);
    Ok(match buf {
        // todo: detect gzip header
        _ => Box::new(flate2::read::GzDecoder::new(new_reader)),
        _ => Box::new(new_reader),
    })
}
*/
