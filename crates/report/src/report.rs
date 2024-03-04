// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use itertools::Itertools;
pub use logjuicer_tokenizer::index_name::IndexName;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use url::Url;

pub mod schema_capnp {
    #![allow(dead_code, unused_qualifications, clippy::extra_unused_type_parameters)]
    include!("../generated/schema_capnp.rs");
}

pub mod codec;
pub mod report_row;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub created_at: SystemTime,
    pub run_time: Duration,
    pub target: Content,
    pub baselines: Vec<Content>,
    pub log_reports: Vec<LogReport>,
    pub index_reports: HashMap<IndexName, IndexReport>,
    pub unknown_files: HashMap<IndexName, Vec<Source>>,
    pub read_errors: Vec<(Source, Box<str>)>,
    pub total_line_count: usize,
    pub total_anomaly_count: usize,
}

impl Report {
    pub fn anomaly_count(&self) -> usize {
        self.log_reports
            .iter()
            .fold(0, |acc, lr| acc + lr.anomalies.len())
    }

    pub fn sample() -> Self {
        use std::{convert::TryInto, ops::Add};
        Report {
            created_at: SystemTime::UNIX_EPOCH.add(Duration::from_secs(42 * 24 * 3600)),
            run_time: Duration::from_secs(42),
            target: Content::File(Source::Local(5, "/proc/status".into())),
            baselines: vec![
                Content::File(Source::Remote(
                    0,
                    "http://localhost/status".try_into().unwrap(),
                )),
                Content::Zuul(Box::new(ZuulBuild::sample("zuul-demo"))),
                Content::Prow(Box::new(ProwBuild::sample("prow-demo"))),
                Content::LocalZuulBuild(
                    "/executor".into(),
                    Box::new(ZuulBuild::sample("local-zuul")),
                ),
            ],
            log_reports: vec![LogReport {
                test_time: Duration::from_secs(84),
                line_count: 1,
                byte_count: 13,
                anomalies: vec![AnomalyContext {
                    before: vec!["before".into(), "...".into()],
                    anomaly: Anomaly {
                        distance: 0.5,
                        pos: 1,
                        timestamp: None,
                        line: "anomaly".into(),
                    },
                    after: vec![],
                }],
                index_name: IndexName("test".into()),
                source: Source::Local(4, "/proc/status".into()),
            }],
            index_reports: HashMap::from([(
                IndexName("i".into()),
                IndexReport {
                    train_time: Duration::from_secs(51),
                    sources: vec![Source::Local(4, "/etc/hosts".into())],
                },
            )]),
            unknown_files: HashMap::from([(
                IndexName("j".into()),
                vec![Source::Remote(
                    0,
                    url::Url::parse("http://local/hosts").unwrap(),
                )],
            )]),
            read_errors: vec![(Source::Local(0, "bad".into()), "oops".into())],
            total_line_count: 42,
            total_anomaly_count: 23,
        }
    }
}

/// The report codec error.
#[derive(Error, Debug)]
pub enum Error {
    #[error("file error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("decode error: {0}")]
    DecodeError(#[from] capnp::Error),
}

impl Report {
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let file = std::fs::File::create(path).map_err(Error::IOError)?;
        if path.to_string_lossy().ends_with(".gz") {
            let dest = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            self.save_writer(dest)
        } else {
            self.save_writer(file)
        }
    }

    pub fn save_writer(&self, dest: impl std::io::Write) -> Result<(), Error> {
        codec::ReportEncoder::new()
            .encode(self, dest)
            .map_err(Error::DecodeError)
    }

    pub fn load(path: &Path) -> Result<Report, Error> {
        let file = std::fs::File::open(path).map_err(Error::IOError)?;
        if path.to_string_lossy().ends_with(".gz") {
            let src = flate2::read::GzDecoder::new(file);
            Self::load_reader(src)
        } else {
            Self::load_reader(file)
        }
    }

    pub fn load_reader(src: impl std::io::Read) -> Result<Report, Error> {
        Self::load_bufreader(std::io::BufReader::new(src))
    }

    pub fn load_bufreader(src: impl std::io::BufRead) -> Result<Report, Error> {
        codec::ReportDecoder::new()
            .decode(src)
            .map_err(Error::DecodeError)
    }

    pub fn load_bytes(data: &[u8]) -> Result<Report, Error> {
        Self::load_reader(data)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZuulBuild {
    pub api: ApiUrl,
    pub uuid: Box<str>,
    pub job_name: Box<str>,
    pub project: Box<str>,
    pub branch: Box<str>,
    pub result: Box<str>,
    pub pipeline: Box<str>,
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
    pub fn sample(name: &str) -> Self {
        Self {
            api: ApiUrl::parse(&format!("http://localhost/{name}-api")).unwrap(),
            uuid: format!("{name}-uuid").into(),
            job_name: format!("{name}-job").into(),
            project: format!("{name}-project").into(),
            branch: format!("{name}-branch").into(),
            result: format!("{name}-result").into(),
            pipeline: format!("{name}-pipeline").into(),
            log_url: Url::parse(&format!("http://localhost/{name}-log")).unwrap(),
            ref_url: Url::parse(&format!("http://localhost/{name}-ref")).unwrap(),
            end_time: codec::read_datetime(name.len() as u64).unwrap(),
            change: name.len() as u64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProwBuild {
    pub url: Url,
    pub uid: Box<str>,
    pub job_name: Box<str>,
    pub project: Box<str>,
    pub pr: u64,
    pub storage_type: Box<str>,
    pub storage_path: Box<str>,
}

impl std::fmt::Display for ProwBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url.as_str())
    }
}

impl ProwBuild {
    pub fn sample(name: &str) -> Self {
        Self {
            url: Url::parse(&format!("http://localhost/{name}-url")).unwrap(),
            uid: format!("{name}-uid").into(),
            job_name: format!("{name}-job").into(),
            project: format!("{name}-project").into(),
            pr: name.len() as u64,
            storage_type: format!("{name}-storage-type").into(),
            storage_path: format!("{name}-storage-path").into(),
        }
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Eq, Hash, PartialOrd, Ord)]
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

    pub fn get_href(&'_ self, content: &Content) -> &'_ str {
        match content {
            Content::LocalZuulBuild(_, _) => self.get_relative(),
            _ => self.as_str(),
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

/// A timestamp in ms since epoch
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Epoch(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anomaly {
    pub distance: f32,
    pub pos: usize,
    pub timestamp: Option<Epoch>,
    pub line: Rc<str>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnomalyContext {
    pub before: Vec<Rc<str>>,
    pub anomaly: Anomaly,
    pub after: Vec<Rc<str>>,
}

impl AnomalyContext {
    pub fn mean(anomalies: &[AnomalyContext]) -> f32 {
        match anomalies.len() {
            0 => 0.0,
            n => anomalies.iter().map(|a| a.anomaly.distance).sum::<f32>() / (n as f32),
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
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

    pub fn timed(&self) -> impl Iterator<Item = (Epoch, &AnomalyContext)> {
        self.anomalies
            .iter()
            .filter_map(|ac| ac.anomaly.timestamp.map(|ts| (ts, ac)))
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
                timestamp: None,
                line: "line".into(),
            },
            before: Vec::new(),
            after: Vec::new(),
        }],
        source: mk_src(name),
        index_name: IndexName::new(),
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

#[derive(Debug, PartialEq, Serialize, Deserialize)]
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
