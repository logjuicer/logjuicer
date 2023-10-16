// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a transparent decompression reader.

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use url::Url;

use std::fs::File;

use crate::env::Env;
use flate2::read::GzDecoder;

/// Handle remote object.
use ureq::{Agent, Response};
mod remote {
    use super::*;

    pub fn get_url(client: &Agent, url: &Url) -> Result<Response> {
        client
            .request_url("GET", url)
            .call()
            .context("Can't get url")
    }

    pub fn head(client: &Agent, url: &Url) -> Result<bool> {
        let resp = client
            .request_url("HEAD", url)
            .call()
            .context("Can't head url")?;
        Ok(is_success(resp.status()))
    }
}

fn is_success(code: u16) -> bool {
    (200..300).contains(&code)
}

// allow large enum for gzdecoder, which are the most used
#[allow(clippy::large_enum_variant)]
pub enum DecompressReader {
    Flat(File),
    Gz(GzDecoder<File>),
    // TODO: support BZIP2 compression
    Remote(UreqReader),
    Cached(logreduce_cache::CacheReader<UreqReader>),
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

pub fn head_url(env: &Env, prefix: usize, url: &Url) -> Result<bool> {
    if let Some(cache) = &env.cache {
        match cache.head(prefix, url) {
            Some(result) => {
                tracing::debug!("Cache hit for {}", url);
                Ok(result)
            }
            None => {
                tracing::debug!("Cache miss for {}", url);
                cache.head_set(prefix, url, remote::head(&env.client, url)?)
            }
        }
    } else {
        remote::head(&env.client, url)
    }
}

/// Read a url, using a prefix size for cache grouping directory.
pub fn from_url(env: &Env, prefix: usize, url: &Url) -> Result<DecompressReader> {
    if let Some(cache) = &env.cache {
        match cache.remote_get(prefix, url) {
            Some(cache) => {
                tracing::debug!("Cache hit for {}", url);
                cache.map(Gz)
            }
            None => {
                tracing::debug!("Cache miss for {}", url);
                let resp = remote::get_url(&env.client, url)?;
                let cache = cache.remote_add(prefix, url, resp.into_reader())?;
                Ok(Cached(cache))
            }
        }
    } else {
        Ok(Remote(remote::get_url(&env.client, url)?.into_reader()))
    }
}

pub fn drop_url(env: &Env, prefix: usize, url: &Url) -> Result<()> {
    if let Some(cache) = &env.cache {
        cache.remote_drop(prefix, url)
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
