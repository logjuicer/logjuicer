// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a transparent decompression reader.

use anyhow::Result;
use flate2::read::GzDecoder;
use liblzma::read::XzDecoder;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use url::Url;

use crate::env::Env;

fn is_success(code: ureq::http::StatusCode) -> bool {
    (200..400).contains(&code.as_u16())
}

pub enum RawReader {
    Local(File),
    Remote(ureq::BodyReader<'static>),
}

pub enum DecompressReader<'a> {
    Raw(RawReader),
    // TODO: support other compression format like bz2
    Gz(GzDecoder<RawReader>),
    Xz(XzDecoder<RawReader>),
    // Checkout the 'with_tarball_source' function for Nested usage
    Nested(Box<dyn Read + 'a>),
}

pub fn from_path(path: &Path) -> Result<DecompressReader<'static>> {
    let reader = RawReader::Local(File::open(path)?);
    let extension = path.extension().unwrap_or_else(|| std::ffi::OsStr::new(""));
    Ok(if extension == "gz" {
        DecompressReader::Gz(GzDecoder::new(reader))
    } else if extension == "xz" {
        DecompressReader::Xz(XzDecoder::new(reader))
    } else {
        DecompressReader::Raw(reader)
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
    let uri = url.as_str();
    tracing::debug!(url = uri, "Requesting url");
    let resp = with_auth(env, env.client.get(uri)).call()?;
    let reader = RawReader::Remote(resp.into_body().into_reader());
    Ok(if uri.ends_with(".xz") {
        DecompressReader::Xz(XzDecoder::new(reader))
    } else {
        // TODO: check that the logserver decompress .gz for us.
        DecompressReader::Raw(reader)
    })
}

impl Read for RawReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            RawReader::Local(r) => r.read(buf),
            RawReader::Remote(r) => r.read(buf),
        }
    }
}

impl Read for DecompressReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            DecompressReader::Raw(r) => r.read(buf),
            DecompressReader::Gz(r) => r.read(buf),
            DecompressReader::Xz(r) => r.read(buf),
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
