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
pub enum DecompressReader<'a> {
    Flat(File),
    Gz(GzDecoder<File>),
    Remote(ureq::BodyReader<'static>),
    Nested(Box<dyn Read + 'a>),
}

pub fn from_path(path: &Path) -> Result<DecompressReader<'static>> {
    let fp = File::open(path)?;
    let extension = path.extension().unwrap_or_else(|| std::ffi::OsStr::new(""));
    Ok(if extension == ".gz" {
        DecompressReader::Gz(GzDecoder::new(fp))
    } else {
        DecompressReader::Flat(fp)
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

pub fn get_url(env: &Env, url: &Url) -> Result<DecompressReader<'static>> {
    tracing::debug!(url = url.as_str(), "Requesting url");
    let resp = with_auth(env, env.client.get(url.as_str())).call()?;
    Ok(DecompressReader::Remote(resp.into_body().into_reader()))
}

impl Read for DecompressReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            DecompressReader::Flat(r) => r.read(buf),
            DecompressReader::Gz(r) => r.read(buf),
            DecompressReader::Remote(r) => r.read(buf),
            DecompressReader::Nested(r) => r.read(buf),
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
