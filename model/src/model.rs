// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a model implementation for the [logreduce](https://github.com/logreduce/logreduce) project.
//!
//! This module dispatch the abstract Content and Source to their implementationm e.g. the files module.

use anyhow::{Context, Result};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use url::Url;

pub mod files;
pub mod process;
mod reader;
pub mod urls;
pub mod zuul;

#[derive(Clone, Copy)]
pub enum OutputMode {
    // Print every steps
    Debug,
    // Print progress using \r
    FastTerminal,
    // Do not print progress, only errors
    Quiet,
}

impl OutputMode {
    pub fn inlined(&self) -> bool {
        matches!(self, OutputMode::FastTerminal)
    }
}

/// The user input.
#[derive(Debug, Serialize, Deserialize)]
pub enum Input {
    Path(String),
    Url(String),
}

impl Input {
    pub fn from_string(s: String) -> Input {
        match s.starts_with("http") {
            true => Input::Url(s),
            false => Input::Path(s),
        }
    }
    pub fn from_pathbuf(s: PathBuf) -> Input {
        Input::Path(s.into_os_string().into_string().unwrap())
    }
}

/// A source of log lines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Content {
    File(Source),
    Directory(Source),
    Zuul(Box<zuul::Build>),
}

impl std::fmt::Display for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Content::File(src) => write!(f, "File({})", src),
            Content::Directory(src) => write!(f, "Directory({})", src),
            Content::Zuul(build) => write!(f, "Zuul({})", build),
        }
    }
}

/// The location of the log lines, and the relative prefix length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Local(usize, PathBuf),
    Remote(usize, url::Url),
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Local(_, _) => write!(f, "local: {}", self.get_relative()),
            Source::Remote(_, _) => write!(f, "remote: {}", self.get_relative()),
        }
    }
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

    fn is_valid(&self) -> bool {
        lazy_static::lazy_static! {
            static ref EXTS: Vec<String> = {
                let mut v = Vec::new();
                for ext in [
                    // binary data with known extension
                    ".ico", ".png", ".clf", ".tar", ".tar.bzip2",
                    ".subunit",
                    ".sqlite", ".db", ".bin", ".pcap.log.txt",
                    // font
                    ".eot", ".otf", ".woff", ".woff2", ".ttf",
                    // config
                    ".yaml", ".ini", ".conf",
                    // not relevant
                    "job-output.json", "log-classify.json", "zuul-manifest.json", ".html",
                    // binary data with known location
                    "cacerts",
                    "local/creds", "pacemaker/authkey",
                    "mysql/tc.log.txt", "corosync/authkey",
                    // swifts
                    "object.builder", "account.builder", "container.builder"
                ] {
                    v.push(ext.to_string());
                    v.push(format!("{}.gz", ext))
                }
                v
            };
        }
        let s = self.as_str();
        EXTS.iter().all(|ext| !s.ends_with(ext)) && !s.contains("/etc/")
    }
}

/// A list of nominal content, e.g. a successful build.
type Baselines = Vec<Content>;

/// An archive of baselines that is used to search anomaly.
#[derive(Debug, Serialize, Deserialize)]
pub struct Model {
    created_at: SystemTime,
    baselines: Baselines,
    indexes: HashMap<IndexName, Index>,
}

/// A LogModelName is an identifier that is used to group similar source.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexName(pub String);

impl std::fmt::Display for IndexName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl IndexName {
    pub fn from_source(source: &Source) -> IndexName {
        IndexName::from_path(source.get_relative())
    }
    pub fn as_str(&self) -> &'_ str {
        self.0.as_str()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    created_at: SystemTime,
    train_time: Duration,
    sources: Vec<Source>,
    index: ChunkIndex,
    line_count: usize,
    byte_count: usize,
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

impl IndexReport {
    pub fn from_index(index: &Index) -> IndexReport {
        IndexReport {
            train_time: index.train_time,
            sources: index.sources.clone(),
        }
    }
}

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

impl Index {
    #[tracing::instrument(level = "debug", name = "Index::train", skip(index))]
    pub fn train(sources: &[Source], mut index: ChunkIndex) -> Result<Index> {
        let created_at = SystemTime::now();
        let start_time = Instant::now();
        let is_json = if let Some(source) = sources.first() {
            source.is_json()
        } else {
            false
        };
        let mut trainer = process::ChunkTrainer::new(&mut index, is_json);
        for source in sources {
            let reader = match source {
                Source::Local(_, path_buf) => Source::file_open(path_buf.as_path())?,
                Source::Remote(prefix, url) => Source::url_open(*prefix, url)?,
            };
            if let Err(e) = trainer.add(reader) {
                tracing::error!("{}: failed to load: {}", source, e)
            }
        }
        trainer.complete();
        let train_time = start_time.elapsed();
        Ok(Index {
            created_at,
            train_time,
            line_count: trainer.line_count,
            byte_count: trainer.byte_count,
            index,
            sources: sources.to_vec(),
        })
    }

    pub fn get_processor<'a>(
        &'a self,
        output_mode: OutputMode,
        source: &Source,
        skip_lines: &'a mut HashSet<String>,
    ) -> Result<process::ChunkProcessor<crate::reader::DecompressReader>> {
        debug_or_progress(output_mode, &format!("Inspecting {}", source));
        let fp = match source {
            Source::Local(_, path_buf) => Source::file_open(path_buf.as_path()),
            Source::Remote(prefix, url) => Source::url_open(*prefix, url),
        }?;
        Ok(process::ChunkProcessor::new(
            fp,
            &self.index,
            source.is_json(),
            skip_lines,
        ))
    }

    #[tracing::instrument(level = "debug", name = "Index::inspect", skip(self, output_mode))]
    pub fn inspect<'a>(
        &'a self,
        output_mode: OutputMode,
        source: &Source,
        skip_lines: &'a mut HashSet<String>,
    ) -> Box<dyn Iterator<Item = Result<AnomalyContext>> + 'a> {
        match self.get_processor(output_mode, source, skip_lines) {
            Ok(processor) => Box::new(processor),
            // If the file can't be open, the first iterator result will be the error.
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }
}

impl Content {
    /// Apply convertion rules to convert the user Input to Content.
    #[tracing::instrument(level = "debug")]
    pub fn from_input(input: Input) -> Result<Content> {
        match input {
            Input::Path(path_str) => Content::from_path(Path::new(&path_str)),
            Input::Url(url_str) => {
                Content::from_url(Url::parse(&url_str).expect("Failed to parse url"))
            }
        }
    }

    /// Create content from raw file path, usefule for testing.
    pub fn from_pathbuf(p: PathBuf) -> Content {
        Content::File(Source::from_pathbuf(p))
    }

    /// Discover the baselines for this Content.
    #[tracing::instrument(level = "debug")]
    pub fn discover_baselines(&self) -> Result<Baselines> {
        (match self {
            Content::File(src) => match src {
                Source::Local(_, pathbuf) => {
                    Content::discover_baselines_from_path(pathbuf.as_path())
                }
                Source::Remote(_, _) => Err(anyhow::anyhow!(
                    "Can't find remmote baselines, they need to be provided"
                )),
            },
            Content::Directory(_) => Err(anyhow::anyhow!(
                "Can't discover directory baselines, they need to be provided",
            )),
            Content::Zuul(build) => build.discover_baselines(),
        })
        .and_then(|baselines| match baselines.len() {
            0 => Err(anyhow::anyhow!("Empty discovered baselines")),
            _ => Ok(baselines),
        })
    }

    /// Get the sources of log lines for this Content.
    #[tracing::instrument(level = "debug")]
    pub fn get_sources(&self) -> Result<Vec<Source>> {
        self.get_sources_iter()
            .filter(|source| {
                source
                    .as_ref()
                    .map(|source| source.is_valid())
                    .unwrap_or(true)
            })
            .collect::<Result<Vec<_>>>()
            .and_then(|sources| match sources.len() {
                0 => Err(anyhow::anyhow!(format!("Empty sources: {}", self))),
                _ => Ok(sources),
            })
    }

    pub fn get_sources_iter(&self) -> Box<dyn Iterator<Item = Result<Source>>> {
        match self {
            Content::File(src) => Box::new(src.file_iter()),
            Content::Directory(src) => match src {
                Source::Local(_, pathbuf) => Box::new(Source::dir_iter(pathbuf.as_path())),
                Source::Remote(_, url) => Box::new(Source::httpdir_iter(url)),
            },
            Content::Zuul(build) => Box::new(build.sources_iter()),
        }
    }

    pub fn group_sources(baselines: &[Content]) -> Result<HashMap<IndexName, Vec<Source>>> {
        let mut groups = HashMap::new();
        for baseline in baselines {
            for source in baseline.get_sources()? {
                groups
                    .entry(IndexName::from_source(&source))
                    .or_insert_with(Vec::new)
                    .push(source);
            }
        }
        Ok(groups)
    }
}

impl Model {
    /// Create a Model from baselines.
    #[tracing::instrument(level = "debug", skip(mk_index, output_mode))]
    pub fn train(
        output_mode: OutputMode,
        baselines: Baselines,
        mk_index: fn() -> ChunkIndex,
    ) -> Result<Model> {
        let created_at = SystemTime::now();
        let mut indexes = HashMap::new();
        for (index_name, sources) in Content::group_sources(&baselines)?.drain() {
            debug_or_progress(
                output_mode,
                &format!(
                    "Loading index {} with {}",
                    index_name,
                    sources.iter().format(", ")
                ),
            );
            let index = Index::train(&sources, mk_index())?;
            indexes.insert(index_name, index);
        }
        Ok(Model {
            created_at,
            baselines,
            indexes,
        })
    }

    pub fn load(path: &Path) -> Result<Model> {
        tracing::info!(path = path.to_str(), "Loading provided model");
        bincode::deserialize_from(flate2::read::GzDecoder::new(
            std::fs::File::open(path).context("Can't open file")?,
        ))
        .context("Can't load model")
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        tracing::info!(path = path.to_str(), "Saving model");
        bincode::serialize_into(
            flate2::write::GzEncoder::new(
                std::fs::File::create(path).context("Can't create file")?,
                flate2::Compression::fast(),
            ),
            self,
        )
        .context("Can't save model")
    }

    /// Get the matching index for a given Source.
    pub fn get_index<'a>(&'a self, index_name: &IndexName) -> Option<&'a Index> {
        lookup_or_single(&self.indexes, index_name)
    }

    /// Create the final report.
    #[tracing::instrument(level = "debug", skip(output_mode, self))]
    pub fn report(&self, output_mode: OutputMode, target: Content) -> Result<Report> {
        let start_time = Instant::now();
        let created_at = SystemTime::now();
        let mut index_reports = HashMap::new();
        let mut log_reports = Vec::new();
        let mut index_errors = Vec::new();
        let mut read_errors = Vec::new();
        let mut total_line_count = 0;
        let mut total_anomaly_count = 0;
        for (index_name, sources) in Content::group_sources(&[target.clone()])?.drain() {
            let mut skip_lines = HashSet::new();
            match self.get_index(&index_name) {
                Some(index) => {
                    for source in sources {
                        let start_time = Instant::now();
                        let mut anomalies = Vec::new();
                        match index.get_processor(output_mode, &source, &mut skip_lines) {
                            Ok(mut processor) => {
                                for anomaly in processor.by_ref() {
                                    match anomaly {
                                        Ok(anomaly) => anomalies.push(anomaly),
                                        Err(err) => {
                                            read_errors.push((source.clone(), format!("{}", err)));
                                            break;
                                        }
                                    }
                                }
                                total_line_count += processor.line_count;
                                if !anomalies.is_empty() {
                                    total_anomaly_count += anomalies.len();
                                    if !index_reports.contains_key(&index_name) {
                                        index_reports.insert(
                                            index_name.clone(),
                                            IndexReport::from_index(index),
                                        );
                                    }
                                    log_reports.push(LogReport {
                                        test_time: start_time.elapsed(),
                                        anomalies,
                                        source,
                                        index_name: index_name.clone(),
                                        line_count: processor.line_count,
                                        byte_count: processor.byte_count,
                                    });
                                }
                            }
                            Err(err) => {
                                read_errors.push((source.clone(), format!("{}", err)));
                                break;
                            }
                        }
                    }
                }
                None => index_errors.push(sources.clone()),
            }
        }
        Ok(Report {
            created_at,
            run_time: start_time.elapsed(),
            target,
            baselines: self.baselines.clone(),
            log_reports,
            index_reports,
            index_errors,
            read_errors,
            total_line_count,
            total_anomaly_count,
        })
    }
}

/// Helper function to debug
pub fn debug_or_progress(output_mode: OutputMode, msg: &str) {
    match output_mode {
        OutputMode::FastTerminal => print!("\r\x1b[1;33m[+]\x1b[0m {}", msg),
        OutputMode::Debug => tracing::debug!("{}", msg),
        OutputMode::Quiet => {}
    }
}

/// Helper function to make a single value hash map always match the key.
/// This is useful when logreduce is used to compare two files which may have different index name.
fn lookup_or_single<'a, K: Eq + std::hash::Hash, V>(hm: &'a HashMap<K, V>, k: &K) -> Option<&'a V> {
    match hm.get(k) {
        None => {
            let values = hm.values().collect::<Vec<&V>>();
            if values.len() == 1 {
                Some(values[0])
            } else {
                None
            }
        }
        Some(v) => Some(v),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ChunkIndex {
    HashingTrick(hashing_index::HashingIndex),
    Noop,
}

/// An API to work with chunks of logs instead of individual line.
impl ChunkIndex {
    fn tokenize(&self, line: &str) -> String {
        match self {
            ChunkIndex::HashingTrick(_) => hashing_index::tokenize(line),
            ChunkIndex::Noop => noop_index::tokenize(line),
        }
    }

    fn add(&mut self, baselines: &[String]) {
        match self {
            ChunkIndex::HashingTrick(i) => i.add(baselines),
            ChunkIndex::Noop => {}
        }
    }

    fn search(&self, targets: &[String]) -> Vec<f32> {
        match self {
            ChunkIndex::HashingTrick(i) => i.search(targets),
            ChunkIndex::Noop => noop_index::search(targets),
        }
    }
}

pub mod hashing_index {
    use serde::{Deserialize, Serialize};
    /// A ChunkIndex implementation.
    #[derive(Debug, Serialize, Deserialize)]
    pub struct HashingIndex {
        baselines: Vec<logreduce_index::FeaturesMatrix>,
    }

    pub fn new() -> super::ChunkIndex {
        super::ChunkIndex::HashingTrick(HashingIndex {
            baselines: Vec::new(),
        })
    }

    pub fn tokenize(line: &str) -> String {
        logreduce_tokenizer::process(line)
    }
    impl HashingIndex {
        pub fn add(&mut self, baselines: &[String]) {
            self.baselines.push(logreduce_index::index_mat(baselines))
        }
        pub fn search(&self, targets: &[String]) -> Vec<f32> {
            logreduce_index::search_mat_chunk(&self.baselines, targets)
        }
    }
}

pub mod noop_index {
    pub fn new() -> super::ChunkIndex {
        super::ChunkIndex::Noop
    }

    /// A ChunkIndex implementation for testing purpose.
    pub fn tokenize(line: &str) -> String {
        line.to_string()
    }

    pub fn search(targets: &[String]) -> Vec<f32> {
        let mut distances = Vec::with_capacity(targets.len());
        distances.resize(targets.len(), 0.0);
        distances
    }
}
