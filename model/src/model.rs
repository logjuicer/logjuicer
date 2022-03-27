// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a model implementation for the [logreduce](https://github.com/logreduce/logreduce) project.
//!
//! This module dispatch the abstract Content and Source to their implementationm e.g. the files module.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use url::Url;

pub mod files;
pub mod process;
mod reader;
pub mod urls;

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
}

/// A source of log lines.
#[derive(Debug, Serialize, Deserialize)]
pub enum Content {
    File(Source),
    Directory(Source),
}

/// The location of the log lines.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Source {
    Local(PathBuf),
    Remote(url::Url),
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
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct IndexName(pub String);

impl IndexName {
    pub fn from_source(source: &Source) -> IndexName {
        match source {
            Source::Local(path_buf) => IndexName::from_path(path_buf.as_path()),
            Source::Remote(url) => IndexName::from_path(Path::new(url.as_str())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    train_time: Duration,
    index: ChunkIndex,
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
    pub anomalies: Vec<AnomalyContext>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub created_at: SystemTime,
    pub targets: Vec<LogReport>,
}

impl Index {
    #[tracing::instrument(name = "Index::train", skip(index))]
    pub fn train(sources: &[Source], mut index: ChunkIndex) -> Result<Index> {
        let start_time = SystemTime::now();
        let mut trainer = process::ChunkTrainer::new(&mut index);
        for source in sources {
            match source {
                Source::Local(path_buf) => trainer.add(Source::file_open(path_buf.as_path())?)?,
                Source::Remote(url) => trainer.add(Source::url_open(url)?)?,
            }
        }
        trainer.complete();
        let train_time = start_time.elapsed().unwrap();
        Ok(Index { train_time, index })
    }

    #[tracing::instrument(name = "Index::inspect", skip(self))]
    pub fn inspect<'a>(
        &'a self,
        source: Source,
    ) -> Box<dyn Iterator<Item = Result<AnomalyContext>> + 'a> {
        match source {
            Source::Local(path_buf) => match Source::file_open(path_buf.as_path()) {
                Ok(fp) => Box::new(process::ChunkProcessor::new(fp, &self.index)),
                // If the file can't be open, the first iterator result will be the error.
                Err(e) => Box::new(std::iter::once(Err(e))),
            },
            Source::Remote(url) => match Source::url_open(&url) {
                Ok(fp) => Box::new(process::ChunkProcessor::new(fp, &self.index)),
                Err(e) => Box::new(std::iter::once(Err(e))),
            },
        }
    }

    // TODO: Implement inspect for multiple sources to share a common skip_lines set
}

impl Content {
    /// Apply convertion rules to convert the user Input to Content.
    #[tracing::instrument]
    pub fn from_input(input: Input) -> Result<Content> {
        match input {
            Input::Path(path_str) => Content::from_path(Path::new(&path_str)),
            Input::Url(url_str) => {
                Content::from_url(Url::parse(&url_str).expect("Failed to parse url"))
            }
        }
    }

    /// Discover the baselines for this Content.
    #[tracing::instrument]
    pub fn discover_baselines(&self) -> Result<Baselines> {
        match self {
            Content::File(src) => match src {
                Source::Local(pathbuf) => Content::discover_baselines_from_path(pathbuf.as_path()),
                Source::Remote(_) => Err(anyhow::anyhow!(
                    "Can't find remmote baselines, they need to be provided"
                )),
            },
            Content::Directory(_) => Err(anyhow::anyhow!(
                "Can't discover directory baselines, they need to be provided",
            )),
        }
    }

    /// Get the sources of log lines for this Content.
    #[tracing::instrument]
    pub fn get_sources(&self) -> Box<dyn Iterator<Item = Result<Source>>> {
        match self {
            Content::File(src) => Box::new(src.file_iter()),
            Content::Directory(src) => match src {
                Source::Local(pathbuf) => Box::new(Source::dir_iter(pathbuf.as_path())),
                Source::Remote(url) => Box::new(Source::httpdir_iter(url)),
            },
        }
    }

    pub fn group_sources(baselines: &[Content]) -> Result<HashMap<IndexName, Vec<Source>>> {
        let mut groups = HashMap::new();
        for baseline in baselines {
            for source in baseline.get_sources() {
                let source = source?;
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
    #[tracing::instrument(skip(mk_index))]
    pub fn train(baselines: Baselines, mk_index: fn() -> ChunkIndex) -> Result<Model> {
        let created_at = SystemTime::now();
        let mut indexes = HashMap::new();
        for (index_name, sources) in Content::group_sources(&baselines)?.drain() {
            let index = Index::train(&sources, mk_index())?;
            indexes.insert(index_name, index);
        }
        Ok(Model {
            created_at,
            baselines,
            indexes,
        })
    }

    /// Get the matching index for a given Source.
    pub fn get_index<'a>(&'a self, source: &Source) -> Option<&'a Index> {
        let index_name = IndexName::from_source(source);
        lookup_or_single(&self.indexes, &index_name)
    }

    /// Create the final report.
    #[tracing::instrument]
    pub fn report(&self, target: &Content) -> Result<Report> {
        let created_at = SystemTime::now();
        let mut targets = Vec::new();
        for source in target.get_sources() {
            let start_time = SystemTime::now();
            let source = source?;
            // TODO: process all the index sources in one pass to share a single skip_lines set.
            let index = self.get_index(&source).expect("Missing baselines");
            let anomalies: Result<Vec<_>> = index.inspect(source).collect();
            let anomalies = anomalies?;
            if !anomalies.is_empty() {
                targets.push(LogReport {
                    test_time: start_time.elapsed().unwrap(),
                    anomalies,
                });
            }
        }
        Ok(Report {
            created_at,
            targets,
        })
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
