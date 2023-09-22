// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides helpers to work with file paths.

use anyhow::{Context, Result};
use std::path::Path;

use crate::env::Env;
use crate::{Baselines, Content, Input, Source};

#[tracing::instrument(level = "debug")]
pub fn content_from_path(path: &Path) -> Result<Content> {
    let src = Source::Local(0, path.to_path_buf());

    if path.is_dir() {
        Ok(Content::Directory(src))
    } else if path.is_file() {
        Ok(Content::File(src))
    } else {
        Err(anyhow::anyhow!("Unknown path: {:?}", path))
    }
}

#[tracing::instrument(level = "debug", skip(env))]
pub fn discover_baselines_from_path(env: &Env, path: &Path) -> Result<Baselines> {
    // TODO: implement discovery by looking for common rotated file names.
    let mut path_str = path.to_path_buf().into_os_string().into_string().unwrap();
    path_str.push_str(".0");
    let baseline = crate::content_from_input(env, Input::Path(path_str))?;
    Ok(vec![baseline])
}

pub fn file_open(path: &Path) -> Result<crate::reader::DecompressReader> {
    tracing::debug!(path = path.to_str(), "Reading file");
    crate::reader::from_path(path).context("Failed to open file")
}

// A file source only has one source
pub fn file_iter(source: &Source) -> impl Iterator<Item = Result<Source>> {
    std::iter::once(Ok(source.clone()))
}

fn keep_path(result: &walkdir::Result<walkdir::DirEntry>) -> bool {
    match result {
        Ok(entry)
            if !entry.path_is_symlink() && entry.file_type().is_file() && !is_hidden(entry) =>
        {
            true
        }
        Ok(_) => false,
        // Keep errors for book keeping
        Err(_) => true,
    }
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

pub fn dir_iter(path: &Path) -> impl Iterator<Item = Result<Source>> {
    let base_len = path.to_str().map(|s| s.len()).unwrap_or(0);
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter(keep_path)
        .map(move |res| match res {
            Err(e) => Err(e.into()),
            Ok(res) => Ok(Source::Local(base_len, res.into_path())),
        })
}
