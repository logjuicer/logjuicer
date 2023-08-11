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

fn contains_vowel(name: &str) -> bool {
    name.contains(|c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y'))
}

fn is_dir_name_irrelevant(name: &str) -> bool {
    is_small_hash(name)
        || !contains_vowel(name)
        || matches!(name, "util" | "tasks" | "manager" | "current")
}

// Return the parent path and it's name.
fn parent_str(path: &Path) -> Option<(&'_ Path, &'_ str)> {
    path.parent().and_then(|parent| {
        parent
            .file_name()
            .and_then(|file_name| file_name.to_str().map(|name| (parent, name)))
    })
}

fn get_parent_name(path: &Path) -> Option<&'_ str> {
    match parent_str(path) {
        None => None,
        // Check if parent is relevant
        Some((_, name)) if !is_dir_name_irrelevant(name) => Some(name),
        // Get the parent's parent
        Some((parent, _)) => get_parent_name(parent),
    }
}

#[test]
fn test_get_parent() {
    assert_eq!(get_parent_name(Path::new("titi/current/log")), Some("titi"));
    assert_eq!(get_parent_name(Path::new("log")), None);
}

fn remove_uid(base: &str) -> String {
    use regex::Regex;
    lazy_static::lazy_static! {
        // ignore components that are 64 char long
        static ref UID: Regex = Regex::new(concat!(
            // Very long continuous word
            r"([0-9a-zA-Z]{63,128}",
            // uuid
            r"|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            r")")).unwrap();
    }
    UID.replace_all(base, "UID").to_string()
}

#[test]
fn test_uid_remove() {
    assert_eq!(
        "UID",
        remove_uid("6339eec3cA2d6a0e36787b10daa5c6513b6ec79933804bd9dcb4c3b59bvwstc")
    )
}

fn remove_non_vowel_component(name: &str) -> String {
    name.split(&['-', '_'])
        .filter(|component| contains_vowel(component))
        .collect::<Vec<&str>>()
        .join("-")
        .to_string()
}

#[test]
fn test_vowel_remove() {
    assert_eq!(
        "test-test".to_string(),
        remove_non_vowel_component("test-fdskl-test")
    )
}

fn clean_name(base: &str) -> String {
    if base.starts_with("instance-00") {
        "instance".to_string()
    } else {
        remove_non_vowel_component(base)
            .replace(
                |c: char| !c.is_ascii_alphabetic() && !matches!(c, '.' | '-'),
                "",
            )
            .trim_matches(|c| matches!(c, '.' | '_' | '-'))
            .trim_end_matches(".gz")
            .to_string()
    }
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
            .unwrap_or("NA");
        let index_name = match get_parent_name(path) {
            None => clean_name(filename),
            Some(name) => format!("{}/{}", clean_name(name), clean_name(filename)),
        };
        IndexName(index_name)
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
        ("builds/log", ["builds/2/log", "builds/42/log"]),
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
