// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a content cache for the [logjuicer](https://github.com/logjuicer/logjuicer) project.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::{Read, Write};
use url::Url;

pub type UrlResult = std::result::Result<Url, Box<str>>;

// Low level functions to create unique file names
mod filename {
    use super::*;
    use sha2::{Digest, Sha256};

    fn new(prefix: char, url: &str) -> String {
        format!("{}{:X}", prefix, Sha256::digest(url))
    }

    fn new_base(prefix: usize, url: &Url) -> String {
        let default_prefix = if prefix == 0 { 23 } else { prefix };
        new('1', &url.as_str()[..default_prefix.min(url.as_str().len())])
    }

    pub fn httpdir(url: &Url) -> String {
        new('0', url.as_str())
    }

    pub fn http(prefix: usize, url: &Url) -> String {
        format!("{}/{}", new_base(prefix, url), new('2', url.as_str()))
    }

    pub fn head_success(prefix: usize, url: &Url) -> String {
        format!("{}/{}", new_base(prefix, url), new('3', url.as_str()))
    }

    pub fn head_failure(prefix: usize, url: &Url) -> String {
        format!("{}/{}", new_base(prefix, url), new('4', url.as_str()))
    }

    pub fn drop(path: Option<std::path::PathBuf>) -> Result<()> {
        path.map_or_else(
            || Ok(()),
            |buf| std::fs::remove_file(buf).context("Failed to delete cache"),
        )
    }
}

/// The Cache object to read and write cached content.
pub struct Cache {
    xdg: xdg::BaseDirectories,
}

impl Cache {
    /// Create the cache.
    pub fn new() -> Result<Cache> {
        xdg::BaseDirectories::with_prefix("logjuicer")
            .map(|xdg| Cache { xdg })
            .context("Failed to get xdg cache directory")
    }

    /// Get a cached head result.
    pub fn head(&self, prefix: usize, path: &Url) -> Option<bool> {
        match self.get(&filename::head_success(prefix, path)) {
            Some(_) => Some(true),
            None => self
                .get(&filename::head_failure(prefix, path))
                .map(|_| false),
        }
    }

    /// Store a head result.
    pub fn head_set(&self, prefix: usize, path: &Url, result: bool) -> Result<bool> {
        let fp = if result {
            filename::head_success(prefix, path)
        } else {
            filename::head_failure(prefix, path)
        };
        self.create(&fp).map(|_| result)
    }

    /// Get a cached file reader.
    pub fn remote_get(&self, prefix: usize, path: &Url) -> Option<Result<GzDecoder<File>>> {
        self.get(&filename::http(prefix, path)).map(|buf| {
            let fp = File::open(buf)?;
            Ok(GzDecoder::new(fp))
        })
    }

    /// Add a file reader to the cache.
    pub fn remote_add<R: Read>(&self, prefix: usize, path: &Url, obj: R) -> Result<CacheReader<R>> {
        let fp = self.create(&filename::http(prefix, path))?;
        Ok(CacheReader {
            remote: obj,
            local: GzEncoder::new(fp, flate2::Compression::fast()),
        })
    }

    /// Get a cached httpdir.
    pub fn httpdir_get(&self, url: &Url) -> Option<Result<Vec<UrlResult>>> {
        self.get(&filename::httpdir(url)).map(|buf| {
            let fp = File::open(buf)?;
            bincode::deserialize_from(fp).context("Failed to decode cached result")
        })
    }

    /// Add a httpdir to the cache.
    pub fn httpdir_add(&self, url: &Url, paths: &[UrlResult]) -> Result<()> {
        let fp = self.create(&filename::httpdir(url))?;
        bincode::serialize_into(fp, paths).context("Failed to serialize httpdir save")
    }

    /// Remove a remote file from the cache.
    pub fn remote_drop(&self, prefix: usize, path: &Url) -> Result<()> {
        filename::drop(self.get(&filename::http(prefix, path)))
    }

    /// Remove a httpdir from the cache.
    pub fn httpdir_drop(&self, url: &Url) -> Result<()> {
        filename::drop(self.get(&filename::httpdir(url)))
    }

    // check if a path exists.
    fn get(&self, path: &str) -> Option<std::path::PathBuf> {
        let buf = self.xdg.get_cache_file(path);
        if buf.as_path().exists() {
            Some(buf)
        } else {
            None
        }
    }

    // creates a new cache entry.
    fn create(&self, path: &str) -> Result<File> {
        let buf = self.xdg.get_cache_file(path);
        let path = buf.as_path();
        if path.exists() {
            Err(anyhow::anyhow!("Cache file already exist: {:?}", buf))
        } else {
            let parent = path.parent().context("Failed to get cache parent")?;
            std::fs::create_dir_all(parent).context("Failed to create parent dir")?;
            File::create(buf).context("Failed to create local cache file")
        }
    }
}

/// A Reader object that saves remote data to a local compressed file.
pub struct CacheReader<R: Read> {
    remote: R,
    local: GzEncoder<File>,
}

impl<R: Read> Read for CacheReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Read remote data.
        let remote_read = self.remote.read(buf);
        match remote_read {
            // Write to the local file
            Ok(remote_size) => self
                .local
                .write_all(&buf[..remote_size])
                .map(|()| remote_size),
            Err(e) => Err(e),
        }
    }
}

#[test]
fn test_cache_httpdir() {
    let cache = Cache::new().unwrap();
    let url = Url::parse("http://localhost/builds").unwrap();
    let paths: Vec<UrlResult> = vec!["job-output.txt", "zuul-info/inventory.yaml"]
        .iter()
        .map(|p| Ok(url.join(p).unwrap()))
        .collect();

    cache.httpdir_drop(&url).unwrap();
    cache.httpdir_add(&url, &paths).unwrap();

    let cached_paths = cache.httpdir_get(&url).unwrap().unwrap();

    assert_eq!(paths, cached_paths);
}

#[test]
fn test_remote() {
    let cache = Cache::new().unwrap();
    let base = Url::parse("http://localhost/builds").unwrap();
    let path = base.join("job-output.txt").unwrap();

    let data = r#"test"#;
    let mut new_data = String::new();

    cache.remote_drop(0, &path).unwrap();
    assert!(
        cache
            .remote_add(0, &path, std::io::Cursor::new(data))
            .unwrap()
            .read_to_string(&mut new_data)
            .unwrap()
            > 0
    );

    assert_eq!(data, new_data);
    new_data.clear();

    assert!(
        cache
            .remote_get(0, &path)
            .unwrap()
            .unwrap()
            .read_to_string(&mut new_data)
            .unwrap()
            > 0
    );
    assert_eq!(data, new_data);
}
