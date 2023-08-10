// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a content cache for the [logreduce](https://github.com/logreduce/logreduce) project.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::{Read, Write};
use url::Url;

// Low level functions to create unique file names
mod filename {
    use super::*;
    use sha2::{Digest, Sha256};

    fn new(prefix: char, url: &Url) -> String {
        format!("{}{:X}", prefix, Sha256::digest(url.as_str()))
    }

    pub fn httpdir(url: &Url) -> String {
        new('0', url)
    }

    pub fn http(base: &Url, url: &Url) -> String {
        format!("{}/{}", new('1', base), new('2', url))
    }

    pub fn head_success(base: &Url, url: &Url) -> String {
        format!("{}/{}", new('1', base), new('3', url))
    }

    pub fn head_failure(base: &Url, url: &Url) -> String {
        format!("{}/{}", new('1', base), new('4', url))
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
        xdg::BaseDirectories::with_prefix("logreduce")
            .map(|xdg| Cache { xdg })
            .context("Failed to get xdg cache directory")
    }

    pub fn head(&self, base: &Url, path: &Url) -> Option<bool> {
        match self.get(&filename::head_success(base, path)) {
            Some(_) => Some(true),
            None => self.get(&filename::head_failure(base, path)).map(|_| false),
        }
    }

    pub fn head_set(&self, base: &Url, path: &Url, result: bool) -> Result<bool> {
        let fp = if result {
            filename::head_success(base, path)
        } else {
            filename::head_failure(base, path)
        };
        self.create(&fp).map(|_| result)
    }

    /// Get a cached file reader.
    pub fn remote_get(&self, base: &Url, path: &Url) -> Option<Result<GzDecoder<File>>> {
        self.get(&filename::http(base, path)).map(|buf| {
            let fp = File::open(buf)?;
            Ok(GzDecoder::new(fp))
        })
    }

    /// Add a file reader to the cache.
    pub fn remote_add<R: Read>(&self, base: &Url, path: &Url, obj: R) -> Result<CacheReader<R>> {
        let fp = self.create(&filename::http(base, path))?;
        Ok(CacheReader {
            remote: obj,
            local: GzEncoder::new(fp, flate2::Compression::fast()),
        })
    }

    /// Get a cached httpdir.
    pub fn httpdir_get(&self, url: &Url) -> Option<Result<Vec<Url>>> {
        self.get(&filename::httpdir(url)).map(|buf| {
            let fp = File::open(buf)?;
            bincode::deserialize_from(fp).context("Failed to decode cached result")
        })
    }

    /// Add a httpdir to the cache.
    pub fn httpdir_add(&self, url: &Url, paths: &[Url]) -> Result<()> {
        let fp = self.create(&filename::httpdir(url))?;
        bincode::serialize_into(fp, paths).context("Failed to serialize httpdir save")
    }

    /// Remove a remote file from the cache.
    pub fn remote_drop(&self, base: &Url, path: &Url) -> Result<()> {
        filename::drop(self.get(&filename::http(base, path)))
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
fn test_httpdir() {
    let cache = Cache::new().unwrap();
    let url = Url::parse("http://localhost/builds").unwrap();
    let paths: Vec<Url> = vec!["job-output.txt", "zuul-info/inventory.yaml"]
        .iter()
        .map(|p| url.join(p).unwrap())
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

    cache.remote_drop(&base, &path).unwrap();
    assert!(
        cache
            .remote_add(&base, &path, std::io::Cursor::new(data))
            .unwrap()
            .read_to_string(&mut new_data)
            .unwrap()
            > 0
    );

    assert_eq!(data, new_data);
    new_data.clear();

    assert!(
        cache
            .remote_get(&base, &path)
            .unwrap()
            .unwrap()
            .read_to_string(&mut new_data)
            .unwrap()
            > 0
    );
    assert_eq!(data, new_data);
}
