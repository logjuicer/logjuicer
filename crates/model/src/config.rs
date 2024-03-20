// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a model configuration.

use logjuicer_report::{Content, Source};
use regex::{Regex, RegexSet};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

mod default_excludes;

/// The loaded user config
pub enum Config {
    /// A single global target config
    Static(TargetConfig),
    /// A list of target config to be matched with the target content
    Matchers(Vec<(MatcherConfig, TargetConfig)>),
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

    /// Convert the raw ConfigFile into a loaded Config
    fn from_config_file(cf: &ConfigFile) -> Result<Self, Error> {
        match cf {
            ConfigFile::Empty => Ok(Config::default()),
            ConfigFile::Static(tcf) => TargetConfig::from_config_file(tcf).map(Config::Static),
            ConfigFile::Matchers(xs) if xs.is_empty() => {
                Err(Error::UnknownFormat("Target list is empty".into()))
            }
            ConfigFile::Matchers(xs) => xs
                .iter()
                .map(|tmf| {
                    Ok((
                        MatcherConfig::from_config_file(tmf)?,
                        TargetConfig::from_config_file(&tmf.config)?,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Config::Matchers),
        }
    }

    /// Get a target config for the given targetr content
    pub fn get_target_config(&self, target: &Content) -> Option<&TargetConfig> {
        match self {
            // When the config is static, use it directly.
            Config::Static(tc) => Some(tc),
            // Otherwise, find the matcher for this target
            Config::Matchers(matchers) => matchers
                .iter()
                .find(|mc| mc.0.matches(target))
                .map(|mc| &mc.1),
        }
    }
}

pub struct TargetConfig {
    includes: Option<RegexSet>,
    excludes: RegexSet,
    skip_duplicate: bool,
    ignore_patterns: RegexSet,
}

impl TargetConfig {
    fn from_config_file(cf: &TargetConfigFile) -> Result<Self, Error> {
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
        Ok(TargetConfig {
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

    pub fn is_ignored_line(&self, line: &str) -> bool {
        self.ignore_patterns.is_match(line)
    }

    pub fn new_skip_lines(&self) -> Option<crate::unordered::KnownLines> {
        if self.skip_duplicate {
            Some(crate::unordered::KnownLines::new())
        } else {
            None
        }
    }
}

pub struct MatcherConfig {
    job_re: Option<Regex>,
}

impl MatcherConfig {
    fn from_config_file(cf: &TargetMatcherFile) -> Result<Self, Error> {
        let job_re = cf.match_job.as_ref().map(|s| Regex::new(s)).transpose()?;
        Ok(MatcherConfig { job_re })
    }

    fn match_job(&self, name: &str) -> bool {
        self.job_re
            .as_ref()
            .map(|job| job.is_match(name))
            .unwrap_or(true)
    }

    fn matches(&self, content: &Content) -> bool {
        match content {
            Content::Zuul(build) => self.match_job(&build.job_name),
            Content::LocalZuulBuild(_, build) => self.match_job(&build.job_name),
            Content::Prow(build) => self.match_job(&build.job_name),
            _ => true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::from_config_file(&ConfigFile::default()).unwrap()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ConfigFile {
    Static(TargetConfigFile),
    Matchers(Vec<TargetMatcherFile>),
    Empty,
}

impl Default for ConfigFile {
    fn default() -> Self {
        ConfigFile::Static(TargetConfigFile::default())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetMatcherFile {
    match_job: Option<String>,
    config: TargetConfigFile,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetConfigFile {
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

impl Default for TargetConfigFile {
    fn default() -> Self {
        TargetConfigFile {
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
    let config = config.get_target_config(&Content::sample("test")).unwrap();
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
        assert_eq!(config_check(&config, src), true);
    }
}

#[cfg(test)]
pub fn config_from_yaml(yaml: &str) -> Config {
    Config::from_reader("config.yaml".into(), std::io::Cursor::new(yaml)).unwrap()
}

#[cfg(test)]
fn config_check(config: &Config, path: &str) -> bool {
    let config = config.get_target_config(&Content::sample("test")).unwrap();
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

#[test]
fn test_config_match() {
    let config = config_from_yaml(
        "
- match_job: config-.*
  config: {}
- match_job: linters
  config:
    ignore_patterns:
    - fetch log
",
    );
    let target_config = |name: &str| config.get_target_config(&Content::sample_job(name));
    assert!(target_config("proj-linters").is_some());
    assert!(target_config("config-check").is_some());
    assert!(target_config("unit").is_none());

    let patterns = target_config("linters").unwrap();
    assert!(patterns.ignore_patterns.is_match("- task: fetch log"));
    assert!(!patterns.ignore_patterns.is_match("traceback"));

    let no_patterns = target_config("config").unwrap();
    assert!(!no_patterns.ignore_patterns.is_match("- task: fetch log"),);
    assert!(!no_patterns.ignore_patterns.is_match("traceback"));
}
