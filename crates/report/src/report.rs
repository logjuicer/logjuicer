// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use itertools::Itertools;
pub use logreduce_tokenizer::index_name::IndexName;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use url::Url;

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub version: String,
    pub created_at: SystemTime,
    pub run_time: Duration,
    pub target: Content,
    pub baselines: Vec<Content>,
    pub log_reports: Vec<LogReport>,
    pub index_reports: HashMap<IndexName, IndexReport>,
    pub unknown_files: HashMap<IndexName, Vec<Source>>,
    pub read_errors: Vec<(Source, String)>,
    pub total_line_count: usize,
    pub total_anomaly_count: usize,
}

/// The report codec error.
#[derive(Error, Debug)]
pub enum Error {
    #[error("file error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("decode error: {0}")]
    DecodeError(#[from] bincode::Error),
}

impl Report {
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        bincode::serialize_into(
            flate2::write::GzEncoder::new(
                std::fs::File::create(path).map_err(Error::IOError)?,
                flate2::Compression::fast(),
            ),
            self,
        )
        .map_err(Error::DecodeError)
    }

    pub fn load(path: &Path) -> Result<Report, Error> {
        bincode::deserialize_from(flate2::read::GzDecoder::new(
            std::fs::File::open(path).map_err(Error::IOError)?,
        ))
        .map_err(Error::DecodeError)
    }

    pub fn load_bytes(data: &[u8]) -> Result<Report, Error> {
        bincode::deserialize_from(flate2::read::GzDecoder::new(data)).map_err(Error::DecodeError)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZuulBuild {
    pub api: ApiUrl,
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

impl ZuulBuild {
    pub fn build_url(&self) -> String {
        let mut url = self
            .api
            .as_str()
            .replace("/tenant/", "/t/")
            .replace("/api", "");
        url.push_str("build/");
        url.push_str(&self.uuid);
        url
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, PartialOrd, Ord)]
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

impl AnomalyContext {
    pub fn mean(anomalies: &[AnomalyContext]) -> f32 {
        match anomalies.len() {
            0 => 0.0,
            n => anomalies.iter().map(|a| a.anomaly.distance).sum::<f32>() / (n as f32),
        }
    }
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

impl LogReport {
    pub fn sorted(log_reports: Vec<LogReport>) -> Vec<LogReport> {
        log_reports
            .into_iter()
            .map(|lr| {
                let mean = if lr.source.get_relative().starts_with("job-output") {
                    // Push job-output to the top
                    42.0
                } else {
                    AnomalyContext::mean(&lr.anomalies)
                };
                (lr, mean)
            })
            .sorted_by(|a, b| b.1.total_cmp(&a.1))
            .map(|(lr, _mean)| lr)
            .collect()
    }
}

#[test]
fn test_report_sort() {
    let mk_src = |name: &str| Source::Local(0, name.into());
    let mk_lr = |name: &str| LogReport {
        test_time: Duration::from_secs(0),
        line_count: 10,
        byte_count: 10,
        anomalies: vec![AnomalyContext {
            anomaly: Anomaly {
                distance: if name == "failure.log" { 0.5 } else { 0.2 },
                pos: 0,
                line: "line".into(),
            },
            before: Vec::new(),
            after: Vec::new(),
        }],
        source: mk_src(name),
        index_name: IndexName("a".into()),
    };
    let reports = vec![
        mk_lr("service.log"),
        mk_lr("job-output.txt.gz"),
        mk_lr("failure.log"),
    ];
    let expected = vec![
        mk_src("job-output.txt.gz"),
        mk_src("failure.log"),
        mk_src("service.log"),
    ];
    let sources: Vec<Source> = LogReport::sorted(reports)
        .into_iter()
        .map(|lr| lr.source)
        .collect();
    assert_eq!(sources, expected);
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexReport {
    pub train_time: Duration,
    pub sources: Vec<Source>,
}

pub fn bytes_to_mb(bytes: usize) -> f64 {
    (bytes as f64) / (1024.0 * 1024.0)
}

/// An url that is guaranteed to be terminated with a slash
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiUrl(Url);

impl From<ApiUrl> for Url {
    fn from(api_url: ApiUrl) -> Url {
        api_url.0
    }
}

impl ApiUrl {
    pub fn into_url(self) -> Url {
        self.0
    }

    pub fn as_url(&self) -> &Url {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn join(&self, input: &str) -> Result<ApiUrl, url::ParseError> {
        if input.ends_with('/') {
            Ok(ApiUrl(self.0.join(input)?))
        } else {
            Err(url::ParseError::Overflow)
        }
    }

    pub fn parse(input: &str) -> Result<ApiUrl, url::ParseError> {
        let url = if input.ends_with('/') {
            Url::parse(input)
        } else {
            Url::parse(&format!("{}/", input))
        };
        Ok(ApiUrl(url?))
    }
}
