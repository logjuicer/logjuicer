// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic to remove noise from file path.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A IndexName is an identifier that is used to group similar source.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexName(pub String);

impl std::fmt::Display for IndexName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
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
    } else if filename.starts_with("pvc-") {
        Some("pvc")
    } else {
        None
    }
}

#[test]
fn test_is_k8s_service() {
    assert_eq!(is_k8s_service("k8s_zuul-uuid"), Some("k8s_zuul"));
    assert_eq!(is_k8s_service("k3s_zuul-uuid"), None);
    assert_eq!(
        is_k8s_service("pvc-328297fa-9941-4df5-b34f-336f02c76be4.txt"),
        Some("pvc")
    );
}

// Handle k8s uuid by dropping everything after the first -
fn take_until_pod_uuid(filename: &str) -> &str {
    if let Some(p) = filename.split('-').next() {
        p
    } else {
        filename
    }
}

#[test]
fn test_take_until_pod_uuid() {
    assert_eq!(
        take_until_pod_uuid("pod/zuul-7fdb57778f-qkzkc.log"),
        "pod/zuul"
    );
    assert_eq!(
        take_until_pod_uuid("pod/nodepool-launcher-fcd58c584-wbcmc.txt"),
        "pod/nodepool"
    )
}

// Return the parent path and it's name.
fn parent_str(path: &Path) -> Option<(&'_ Path, &'_ str)> {
    path.parent().and_then(|parent| {
        parent
            .file_name()
            .and_then(|file_name| file_name.to_str().map(|name| (parent, name)))
    })
}

fn remove_uid(base: &str) -> String {
    lazy_static::lazy_static! {
        // ignore components that are 64 char long
        static ref RE: regex::Regex = regex::Regex::new("[a-zA-Z0-9]{63,64}").unwrap();
    }
    RE.replace_all(base, "UID").to_string()
}

#[test]
fn test_uid_remove() {
    assert_eq!(
        "UID",
        remove_uid("6339eec3cA2d6a0e36787b10daa5c6513b6ec79933804bd9dcb4c3b59bvwstc")
    )
}

impl IndexName {
    /// Retrieves the underlying str.
    pub fn as_str(&self) -> &'_ str {
        self.0.as_str()
    }

    /// Creates IndexName from a path.
    pub fn from_path(base: &str) -> IndexName {
        let base_no_id = remove_uid(base);
        let path = Path::new(&base_no_id);
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
        } else if shortfilename.starts_with("pod/") {
            take_until_pod_uuid(&shortfilename).to_string()
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
        (
            "pod/UID.txt",
            [
                "pod/6339eec3ca2d6a0e36787b10daa5c6513b6ec79933804bd9dcb4c3b59bvwstc.txt",
                "pod/6339eec3cA2d6a0e36787b10daa5c6513b6ec79933804bd9dcb4c3b59bvwstc.txt",
            ],
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
