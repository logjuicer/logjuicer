// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a model configuration.

use logjuicer_report::Source;
use regex::RegexSet;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

mod default_excludes;

pub struct Config {
    includes: Option<RegexSet>,
    excludes: RegexSet,
    skip_duplicate: bool,
    pub ignore_patterns: RegexSet,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("bad regex: {0}")]
    BadRegex(#[from] regex::Error),

    #[error("invalid file: {0}")]
    BadFile(#[from] std::io::Error),

    #[error("invalid json: {0}")]
    BadJSON(#[from] serde_json::Error),

    #[error("invalid yaml: {0}")]
    BadYAML(#[from] serde_yaml::Error),

    #[error("unknown format: {0}")]
    UnknownFormat(String),
}

impl Config {
    pub fn from_path(path: PathBuf) -> Result<Self, Error> {
        let file = std::fs::File::open(&path)?;
        Config::from_reader(path, file)
    }

    fn from_reader<R: std::io::Read>(path: PathBuf, file: R) -> Result<Self, Error> {
        let reader = std::io::BufReader::new(file);
        let cf = match path.as_path().extension().and_then(std::ffi::OsStr::to_str) {
            Some("yaml") => Ok(serde_yaml::from_reader(reader)?),
            Some("json") => Ok(serde_json::from_reader(reader)?),
            m_ext => Err(Error::UnknownFormat(
                m_ext.map(|s| s.to_string()).unwrap_or("".to_string()),
            )),
        }?;
        Config::from_config_file(&cf)
    }

    fn from_config_file(cf: &ConfigFile) -> Result<Self, Error> {
        let includes = if cf.includes.is_empty() {
            None
        } else {
            Some(RegexSet::new(&cf.includes)?)
        };
        let excludes = if cf.default_excludes {
            RegexSet::new(
                cf.excludes.iter().map(|s| s as &str).chain(
                    crate::config::default_excludes::DEFAULT_EXCLUDES
                        .iter()
                        .map(|s| s as &str),
                ),
            )
        } else {
            RegexSet::new(&cf.excludes)
        }?;
        let ignore_patterns = RegexSet::new(&cf.ignore_patterns)?;
        let skip_duplicate = if std::env::var("LOGJUICER_KEEP_DUPLICATE").is_ok() {
            false
        } else {
            cf.skip_duplicate
        };
        Ok(Config {
            includes,
            excludes,
            skip_duplicate,
            ignore_patterns,
        })
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

    pub fn new_skip_lines(&self) -> Option<crate::unordered::KnownLines> {
        if self.skip_duplicate {
            Some(crate::unordered::KnownLines::new())
        } else {
            None
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::from_config_file(&ConfigFile::default()).unwrap()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    includes: Vec<String>,
    #[serde(default)]
    excludes: Vec<String>,
    #[serde(default = "default_default_excludes")]
    default_excludes: bool,
    #[serde(default = "default_default_excludes")]
    skip_duplicate: bool,
    #[serde(default)]
    ignore_patterns: Vec<String>,
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
            skip_duplicate: true,
            ignore_patterns: Vec::new(),
        }
    }
}

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
    let config = config_from_yaml("");
    for src in ["service/api.log", "job-output.txt"] {
        let source = Source::from_pathbuf(src.into());
        assert_eq!(config.is_source_valid(&source), true,)
    }
}

#[cfg(test)]
fn config_from_yaml(yaml: &str) -> Config {
    Config::from_reader("config.yaml".into(), std::io::Cursor::new(yaml)).unwrap()
}

#[cfg(test)]
fn config_check(config: &Config, path: &str) -> bool {
    config.is_source_valid(&Source::from_pathbuf(path.into()))
}

#[test]
fn test_config_include() {
    let config = config_from_yaml(
        "
includes:
  - undercloud/deploy.log
",
    );
    assert_eq!(config_check(&config, "service/api.log"), false);
    assert_eq!(config_check(&config, "undercloud/deploy.log"), true);
    assert_eq!(config_check(&config, "undercloud/deploy.log.log"), true);
    assert_eq!(config_check(&config, "undercloud/deploy.log.png"), false);
}

#[test]
fn test_config_exclude() {
    let config = config_from_yaml(
        "
excludes:
  - bzImage
",
    );
    assert_eq!(config_check(&config, "deploy/bzImage.gz"), false);
    assert_eq!(config_check(&config, "test.png"), false);
    assert_eq!(config_check(&config, "undercloud/deploy.log"), true);
}

#[test]
fn test_config_no_default() {
    let config = config_from_yaml(
        "
default_excludes: false
excludes:
  - bzImage
",
    );
    assert_eq!(config_check(&config, "test.png"), true);
    assert_eq!(config_check(&config, "/.git/config"), true);
    assert_eq!(config_check(&config, "boot/bzImage"), false);
}

#[test]
fn test_config_bad() {
    assert_eq!(
        true,
        Config::from_reader("config.yaml".into(), std::io::Cursor::new("unknown: true")).is_err()
    );
    assert_eq!(
        true,
        Config::from_reader(
            "config.json".into(),
            std::io::Cursor::new("{\"unknown\": true}")
        )
        .is_err()
    );
}
