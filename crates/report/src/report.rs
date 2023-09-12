// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
pub use logreduce_tokenizer::index_name::IndexName;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use url::Url;

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub created_at: SystemTime,
    pub run_time: Duration,
    pub target: Content,
    pub baselines: Vec<Content>,
    pub log_reports: Vec<LogReport>,
    pub index_reports: HashMap<IndexName, IndexReport>,
    pub index_errors: Vec<Vec<Source>>,
    pub read_errors: Vec<(Source, String)>,
    pub total_line_count: usize,
    pub total_anomaly_count: usize,
}

impl Report {
    pub fn save(&self, path: &Path) -> Result<()> {
        bincode::serialize_into(
            flate2::write::GzEncoder::new(
                std::fs::File::create(path).context("Can't create report file")?,
                flate2::Compression::fast(),
            ),
            self,
        )
        .context("Can't save report")
    }

    pub fn load(path: &Path) -> Result<Report> {
        bincode::deserialize_from(flate2::read::GzDecoder::new(
            std::fs::File::open(path).context("Can't open report file")?,
        ))
        .context("Can't load report")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZuulBuild {
    pub api: Url,
    pub per_project: bool,
    pub uuid: String,
    pub job_name: String,
    pub project: String,
    pub branch: String,
    pub result: String,
    pub pipeline: String,
    pub log_url: Url,
    pub ref_url: Url,
    pub end_time: DateTime<Utc>,
    pub change: u64,
}

impl std::fmt::Display for ZuulBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}build/{}", self.api.as_str(), self.uuid)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProwBuild {
    pub url: Url,
    pub uid: String,
    pub job_name: String,
    pub project: String,
    pub pr: u64,
    pub storage_type: String,
    pub storage_path: String,
}

impl std::fmt::Display for ProwBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url.as_str())
    }
}

/// A source of log lines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Content {
    File(Source),
    Directory(Source),
    Zuul(Box<ZuulBuild>),
    Prow(Box<ProwBuild>),
    LocalZuulBuild(PathBuf, Box<ZuulBuild>),
}

impl std::fmt::Display for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Content::File(src) => write!(f, "File({})", src),
            Content::Directory(src) => write!(f, "Directory({})", src),
            Content::Zuul(build) => write!(f, "Zuul({})", build),
            Content::Prow(build) => write!(f, "Prow({})", build.url.as_str()),
            Content::LocalZuulBuild(src, _build) => {
                write!(f, "LocalZuulBuild({:?})", src.as_os_str())
            }
        }
    }
}

/// The location of the log lines, and the relative prefix length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Local(usize, PathBuf),
    Remote(usize, Url),
}

impl Source {
    pub fn from_pathbuf(p: PathBuf) -> Source {
        Source::Local(0, p)
    }
    pub fn is_json(&'_ self) -> bool {
        self.get_relative().ends_with(".json")
    }
    pub fn get_relative(&'_ self) -> &'_ str {
        match self {
            Source::Local(base_len, path) => &path.to_str().unwrap_or("")[*base_len..],
            Source::Remote(base_len, url) => &url.as_str()[*base_len..],
        }
    }

    pub fn as_str(&'_ self) -> &'_ str {
        match self {
            Source::Local(_, path) => path.to_str().unwrap_or(""),
            Source::Remote(_, url) => url.as_str(),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Local(_, _) => write!(f, "local: {}", self.get_relative()),
            Source::Remote(_, _) => write!(f, "remote: {}", self.get_relative()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Anomaly {
    pub distance: f32,
    pub pos: usize,
    pub line: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnomalyContext {
    pub before: Vec<String>,
    pub anomaly: Anomaly,
    pub after: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogReport {
    pub test_time: Duration,
    pub line_count: usize,
    pub byte_count: usize,
    pub anomalies: Vec<AnomalyContext>,
    pub source: Source,
    pub index_name: IndexName,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexReport {
    pub train_time: Duration,
    pub sources: Vec<Source>,
}
