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

fn is_success(code: u16) -> bool {
    (200..400).contains(&code)
}

// allow large enum for gzdecoder, which are the most used
#[allow(clippy::large_enum_variant)]
pub enum DecompressReader {
    Flat(File),
    Gz(GzDecoder<File>),
    // TODO: support BZIP2 compression
    Remote(UreqReader),
}
use DecompressReader::*;

type UreqReader = Box<dyn Read + Send + Sync + 'static>;

pub fn from_path(path: &Path) -> Result<DecompressReader> {
    let fp = File::open(path)?;
    let extension = path.extension().unwrap_or_else(|| std::ffi::OsStr::new(""));
    Ok(if extension == ".gz" {
        Gz(GzDecoder::new(fp))
    } else {
        Flat(fp)
    })
}

pub fn head_url(env: &Env, url: &Url) -> Result<bool> {
    let resp = env.client.request_url("HEAD", url).call()?;
    Ok(is_success(resp.status()))
}

pub fn get_url(env: &Env, url: &Url) -> Result<DecompressReader> {
    let resp = env.client.request_url("GET", url).call()?;
    Ok(Remote(resp.into_reader()))
}

impl Read for DecompressReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // TODO: refactor using the enum_dispatch crate.
        match self {
            Flat(r) => r.read(buf),
            Gz(r) => r.read(buf),
            Remote(r) => r.read(buf),
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
