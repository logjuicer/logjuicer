// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a transparent decompression reader.

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use url::Url;

use std::fs::File;

use flate2::read::GzDecoder;

// TODO: use a struct to pass these references.
lazy_static::lazy_static! {
    static ref CACHE: logreduce_cache::Cache = logreduce_cache::Cache::new().expect("Cache");
    static ref CLIENT: reqwest::blocking::Client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(std::env::var("LOGREDUCE_SSL_NO_VERIFY").is_ok())
        .build()
        .expect("Client");

    static ref USE_CACHE: bool = std::env::var("LOGREDUCE_CACHE").is_ok();
}

/// Handle remote object.
use reqwest::blocking::Response;
mod remote {
    use super::*;

    pub fn get_url(url: &Url) -> Result<Response> {
        CLIENT.get(url.clone()).send().context("Can't get url")
    }

    pub fn head(url: &Url) -> Result<bool> {
        let resp = CLIENT.head(url.clone()).send().context("Can't head url")?;
        Ok(resp.status().is_success())
    }
}

// allow large enum for gzdecoder, which are the most used
#[allow(clippy::large_enum_variant)]
pub enum DecompressReader {
    Flat(File),
    Gz(GzDecoder<File>),
    // TODO: support BZIP2 compression
    Remote(Response),
    Cached(logreduce_cache::CacheReader<Response>),
}
use DecompressReader::*;

pub fn from_path(path: &Path) -> Result<DecompressReader> {
    let fp = File::open(path)?;
    let extension = path.extension().unwrap_or_else(|| std::ffi::OsStr::new(""));
    Ok(if extension == ".gz" {
        Gz(GzDecoder::new(fp))
    } else {
        Flat(fp)
    })
}

pub fn head_url(base: &Url, url: &Url) -> Result<bool> {
    if *USE_CACHE {
        match CACHE.head(base, url) {
            Some(result) => {
                tracing::debug!("Cache hit for {}", url);
                Ok(result)
            }
            None => {
                tracing::debug!("Cache miss for {}", url);
                CACHE.head_set(base, url, remote::head(url)?)
            }
        }
    } else {
        remote::head(url)
    }
}

pub fn from_url(base: &Url, url: &Url) -> Result<DecompressReader> {
    if *USE_CACHE {
        match CACHE.remote_get(base, url) {
            Some(cache) => {
                tracing::debug!("Cache hit for {}", url);
                cache.map(Gz)
            }
            None => {
                tracing::debug!("Cache miss for {}", url);
                let resp = remote::get_url(url)?;
                let cache = CACHE.remote_add(base, url, resp)?;
                Ok(Cached(cache))
            }
        }
    } else {
        Ok(Remote(remote::get_url(url)?))
    }
}

pub fn drop_url(base: &Url, url: &Url) -> Result<()> {
    if *USE_CACHE {
        CACHE.remote_drop(base, url)
    } else {
        Ok(())
    }
}

impl Read for DecompressReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // TODO: refactor using the enum_dispatch crate.
        match self {
            Flat(r) => r.read(buf),
            Gz(r) => r.read(buf),
            Remote(r) => r.read(buf),
            Cached(r) => r.read(buf),
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
