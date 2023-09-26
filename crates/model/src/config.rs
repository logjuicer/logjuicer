// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a model configuration.

use logreduce_report::Source;
use regex::RegexSet;
use serde::{Deserialize, Serialize};

pub struct Config {
    includes: Option<RegexSet>,
    excludes: RegexSet,
}

impl Config {
    fn from_config_file(cf: &ConfigFile) -> Result<Self, regex::Error> {
        let includes = if cf.includes.is_empty() {
            None
        } else {
            Some(RegexSet::new(&cf.includes)?)
        };
        let excludes = if cf.default_excludes {
            RegexSet::new(
                cf.excludes
                    .iter()
                    .map(|s| s as &str)
                    .chain(DEFAULT_EXCLUDES.iter().map(|s| s as &str)),
            )
        } else {
            RegexSet::new(&cf.excludes)
        }?;
        Ok(Config { includes, excludes })
    }

    pub fn is_source_valid(&self, source: &Source) -> bool {
        let fp = source.get_relative().trim_end_matches(".gz");
        if let Some(includes) = &self.includes {
            if !includes.is_match(fp) {
                return false;
            }
        }
        !self.excludes.is_match(fp)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::from_config_file(&ConfigFile::default()).unwrap()
    }
}

#[derive(Serialize, Deserialize)]
struct ConfigFile {
    includes: Vec<String>,
    excludes: Vec<String>,
    #[serde(default = "default_default_excludes")]
    default_excludes: bool,
}

fn default_default_excludes() -> bool {
    true
}

impl Default for ConfigFile {
    fn default() -> Self {
        ConfigFile {
            includes: Vec::new(),
            excludes: Vec::new(),
            default_excludes: true,
        }
    }
}

const DEFAULT_EXCLUDES: &[&str] = &[
    // binary data with known extension
    ".ico$",
    ".png$",
    ".clf$",
    ".tar$",
    ".tar.bzip2$",
    ".subunit$",
    ".sqlite$",
    ".db$",
    ".bin$",
    ".pcap.log.txt$",
    // font
    ".eot$",
    ".otf$",
    ".woff$",
    ".woff2$",
    ".ttf$",
    // config
    ".yaml$",
    ".ini$",
    ".conf$",
    // not relevant
    "job-output.json$",
    "zuul-manifest.json$",
    ".html$",
    // binary data with known location
    "cacerts$",
    "local/creds$",
    "/authkey$",
    "mysql/tc.log.txt$",
    // swifts
    "object.builder$",
    "account.builder$",
    "container.builder$",
    // system config
    "/etc/",
    // hidden files
    "/\\.",
];

#[test]
fn test_config_default_exclude() {
    let config = Config::default();
    for src in [
        "config.yaml",
        "/config/.git/HEAD",
        "font.ttf.gz",
        "/system/etc/conf",
    ] {
        let source = Source::from_pathbuf(src.into());
        assert_eq!(config.is_source_valid(&source), false)
    }
}

#[test]
fn test_config_default() {
    let config = Config::default();
    for src in ["service/api.log", "job-output.txt"] {
        let source = Source::from_pathbuf(src.into());
        assert_eq!(config.is_source_valid(&source), true,)
    }
}
