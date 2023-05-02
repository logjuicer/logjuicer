// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides helpers to work with file paths.

use anyhow::{Context, Result};
use std::path::Path;

use crate::{Baselines, Content, IndexName, Input, Source};

impl Content {
    #[tracing::instrument(level = "debug")]
    pub fn from_path(path: &Path) -> Result<Content> {
        let src = Source::Local(0, path.to_path_buf());

        if path.is_dir() {
            Ok(Content::Directory(src))
        } else if path.is_file() {
            Ok(Content::File(src))
        } else {
            Err(anyhow::anyhow!("Unknown path: {:?}", path))
        }
    }

    #[tracing::instrument(level = "debug")]
    pub fn discover_baselines_from_path(path: &Path) -> Result<Baselines> {
        // TODO: implement discovery by looking for common rotated file names.
        let mut path_str = path.to_path_buf().into_os_string().into_string().unwrap();
        path_str.push_str(".0");
        let baseline = Content::from_input(Input::Path(path_str))?;
        Ok(vec![baseline])
    }
}

impl Source {
    pub fn file_open(path: &Path) -> Result<crate::reader::DecompressReader> {
        tracing::debug!(path = path.to_str(), "Reading file");
        crate::reader::from_path(path).context("Failed to open file")
    }

    // A file source only has one source
    pub fn file_iter(&self) -> impl Iterator<Item = Result<Source>> {
        std::iter::once(Ok(self.clone()))
    }

    fn keep_path(result: &walkdir::Result<walkdir::DirEntry>) -> bool {
        match result {
            Ok(entry) if !entry.path_is_symlink() && entry.file_type().is_file() => true,
            Ok(_) => false,
            // Keep errors for book keeping
            Err(_) => true,
        }
    }

    pub fn dir_iter(path: &Path) -> impl Iterator<Item = Result<Source>> {
        let base_len = path.to_str().map(|s| s.len()).unwrap_or(0);
        walkdir::WalkDir::new(path)
            .into_iter()
            .filter(Source::keep_path)
            .map(move |res| match res {
                Err(e) => Err(e.into()),
                Ok(res) => Ok(Source::Local(base_len, res.into_path())),
            })
    }
}

fn is_small_hash(filename: &str) -> bool {
    filename.len() == 7
        && !filename.contains(|c: char| !('a'..='f').contains(&c) && !c.is_ascii_digit())
}

#[test]
fn test_is_small_hash() {
    assert!(is_small_hash("015da2b"));
    assert!(!is_small_hash("abcda2z"));
    assert_eq!(
        IndexName::from_path("config-update/015da2b/job-output.json.gz"),
        IndexName("config-update/job-output.json".to_string())
    )
}

fn is_k8s_service(filename: &str) -> Option<&str> {
    if filename.starts_with("k8s_") {
        match filename.split_once('-') {
            Some((service, _uuid)) => Some(service),
            None => None,
        }
    } else {
        None
    }
}

#[test]
fn test_is_k8s_service() {
    assert_eq!(is_k8s_service("k8s_zuul-uuid"), Some("k8s_zuul"));
    assert_eq!(is_k8s_service("k3s_zuul-uuid"), None);
}

// Return the parent path and it's name.
fn parent_str(path: &Path) -> Option<(&'_ Path, &'_ str)> {
    path.parent().and_then(|parent| {
        parent
            .file_name()
            .and_then(|file_name| file_name.to_str().map(|name| (parent, name)))
    })
}

impl IndexName {
    pub fn from_path(base: &str) -> IndexName {
        let path = Path::new(base);
        let filename: &str = path
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .unwrap_or("N/A");
        // shortfilename is the filename with it's first parent directory name
        let shortfilename: String = match parent_str(path) {
            None => filename.to_string(),
            Some((parent, name)) if is_small_hash(name) => format!(
                "{}/{}",
                parent_str(parent).map(|(_, name)| name).unwrap_or(""),
                filename
            ),
            Some((_, name)) => format!("{}/{}", name, filename),
        };

        let model_name = if shortfilename.starts_with("qemu/instance-") {
            "qemu/instance".to_string()
        } else if let Some(service) = is_k8s_service(filename) {
            service.to_string()
        } else {
            // removes number and symbols
            shortfilename
                .replace(
                    |c: char| !c.is_ascii_alphabetic() && !matches!(c, '/' | '.' | '_' | '-'),
                    "",
                )
                .trim_matches(|c| matches!(c, '/' | '.' | '_' | '-'))
                .trim_end_matches(".gz")
                .to_string()
        };
        IndexName(model_name)
    }
}

#[test]
fn log_model_name() {
    IntoIterator::into_iter([
        (
            "qemu/instance",
            [
                "containers/libvirt/qemu/instance-0000001d.log.txt.gz",
                "libvirt/qemu/instance-000000ec.log.txt.gz",
            ],
        ),
        ("log", ["builds/2/log", "42/log"]),
        ("audit/audit.log", ["audit/audit.log", "audit/audit.log.1"]),
        (
            "zuul/merger.log",
            ["zuul/merger.log", "zuul/merger.log.2017-11-12"],
        ),
    ])
    .for_each(|(expected_model, paths)| {
        IntoIterator::into_iter(paths).for_each(|path| {
            assert_eq!(
                IndexName(expected_model.to_string()),
                IndexName::from_path(path),
                "for {}",
                path
            )
        })
    });
}
