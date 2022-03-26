// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides helpers to work with file paths.

use std::fs::File;
use std::io::Result;
use std::path::Path;

use crate::{Baselines, Content, IndexName, Input, Source};

impl Content {
    #[tracing::instrument]
    pub fn from_path(path: &Path) -> Result<Content> {
        let src = Source::Local(path.to_path_buf());

        if path.is_dir() {
            Ok(Content::Directory(src))
        } else if path.is_file() {
            Ok(Content::File(src))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unknown path {}", path.to_string_lossy()),
            ))
        }
    }

    #[tracing::instrument]
    pub fn discover_baselines_from_path(path: &Path) -> Result<Baselines> {
        // TODO: implement discovery by looking for common rotated file names.
        let mut path_str = path.to_path_buf().into_os_string().into_string().unwrap();
        path_str.push_str(".0");
        let baseline = Content::from_input(Input::Path(path_str))?;
        Ok(vec![baseline])
    }
}

impl Source {
    pub fn file_open(path: &Path) -> Result<File> {
        File::open(path)
    }

    // A file source only has one source
    pub fn file_iter(&self) -> impl Iterator<Item = Result<Source>> {
        std::iter::once(Ok(self.clone()))
    }

    pub fn dir_iter(&self) -> impl Iterator<Item = Result<Source>> {
        match self {
            Source::Local(path_buf) => Source::local_dir_iter(path_buf.as_path()),
        }
    }

    fn keep_path(result: &walkdir::Result<walkdir::DirEntry>) -> bool {
        match result {
            Ok(entry) if !entry.path_is_symlink() && entry.file_type().is_file() => true,
            Ok(_) => false,
            // Keep errors for book keeping
            Err(_) => true,
        }
    }

    fn local_dir_iter(path: &Path) -> impl Iterator<Item = Result<Source>> {
        walkdir::WalkDir::new(path)
            .into_iter()
            .filter(Source::keep_path)
            .map(|res| match res {
                Err(e) => Err(e.into()),
                Ok(res) => Ok(Source::Local(res.into_path())),
            })
    }
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

impl IndexName {
    pub fn from_path(path: &Path) -> IndexName {
        let filename: &str = path
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .unwrap_or("N/A");
        // shortfilename is the filename with it's first parent directory name
        let shortfilename: String = match path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|os_str| os_str.to_str())
        {
            None => filename.to_string(),
            Some(parent) => format!("{}/{}", parent, filename),
        };

        let model_name = if shortfilename.starts_with("qemu/instance-") {
            "qemu/instance".to_string()
        } else if let Some(service) = is_k8s_service(filename) {
            service.to_string()
        } else {
            // TODO: add zuul job pipeline name from upper in the path (e.g. post/uid/job-name/uuid/.../logfile)
            // removes number and symbols
            shortfilename
                .replace(
                    |c: char| !c.is_ascii_alphabetic() && !matches!(c, '/' | '.' | '_' | '-'),
                    "",
                )
                .trim_matches(|c| matches!(c, '/' | '.' | '_' | '-'))
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
        IntoIterator::into_iter(paths)
            .map(Path::new)
            .for_each(|path| {
                assert_eq!(
                    IndexName(expected_model.to_string()),
                    IndexName::from_path(path),
                    "for {}",
                    path.display()
                )
            })
    });
}
