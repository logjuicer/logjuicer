// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a model implementation for the [logreduce](https://github.com/logreduce/logreduce) project.
//!
//! This module dispatch the abstract Content and Source to their implementationm e.g. the files module.

use anyhow::{Context, Result};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use url::Url;

pub use logreduce_tokenizer::index_name::IndexName;

pub use logreduce_report::{AnomalyContext, IndexReport, LogReport, Report, Source, ZuulBuild};

use crate::env::Env;
use crate::files::{dir_iter, file_iter, file_open};
use crate::unordered::KnownLines;
use crate::urls::{httpdir_iter, url_open};
pub mod env;
pub mod files;
pub mod process;
pub mod prow;
mod reader;
pub mod unordered;
pub mod urls;
pub mod zuul;

const MODEL_MAGIC: &str = "LGRD";

// Remember to bump this value when changing the tokenizer or the vectorizer to avoid using incompatible models.
const MODEL_VERSION: usize = 2;

/// The user input.
#[derive(Debug, Serialize, Deserialize)]
pub enum Input {
    Path(String),
    Url(String),
    ZuulBuild(PathBuf, String, bool),
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
    Zuul(Box<ZuulBuild>),
    Prow(Box<prow::Build>),
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

fn is_source_valid(source: &Source) -> bool {
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
    let s = source.as_str();
    EXTS.iter().all(|ext| !s.ends_with(ext)) && !s.contains("/etc/")
}

/// A list of nominal content, e.g. a successful build.
type Baselines = Vec<Content>;

/// An archive of baselines that is used to search anomaly.
#[derive(Debug, Serialize, Deserialize)]
pub struct Model {
    pub created_at: SystemTime,
    pub baselines: Baselines,
    pub indexes: HashMap<IndexName, Index>,
}

pub fn indexname_from_source(source: &Source) -> IndexName {
    IndexName::from_path(source.get_relative())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    pub created_at: SystemTime,
    pub train_time: Duration,
    pub sources: Vec<Source>,
    index: ChunkIndex,
    pub line_count: usize,
    pub byte_count: usize,
}

impl Index {
    pub fn to_report(&self) -> IndexReport {
        IndexReport {
            train_time: self.train_time,
            sources: self.sources.clone(),
        }
    }
}

impl Index {
    #[tracing::instrument(level = "debug", name = "Index::train", skip(env, index))]
    pub fn train(env: &Env, sources: &[Source], mut index: ChunkIndex) -> Result<Index> {
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
                Source::Local(_, path_buf) => file_open(path_buf.as_path())?,
                Source::Remote(prefix, url) => url_open(env, *prefix, url)?,
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
        env: &Env,
        source: &Source,
        skip_lines: &'a mut KnownLines,
    ) -> Result<process::ChunkProcessor<crate::reader::DecompressReader>> {
        env.debug_or_progress(&format!("Inspecting {}", source));
        let fp = match source {
            Source::Local(_, path_buf) => file_open(path_buf.as_path()),
            Source::Remote(prefix, url) => url_open(env, *prefix, url),
        }?;
        Ok(process::ChunkProcessor::new(
            fp,
            &self.index,
            source.is_json(),
            skip_lines,
        ))
    }

    #[tracing::instrument(level = "debug", name = "Index::inspect", skip(self, env))]
    pub fn inspect<'a>(
        &'a self,
        env: &Env,
        source: &Source,
        skip_lines: &'a mut KnownLines,
    ) -> Box<dyn Iterator<Item = Result<AnomalyContext>> + 'a> {
        match self.get_processor(env, source, skip_lines) {
            Ok(processor) => Box::new(processor),
            // If the file can't be open, the first iterator result will be the error.
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }
}

impl Content {
    /// Apply convertion rules to convert the user Input to Content.
    #[tracing::instrument(level = "debug", skip(env))]
    pub fn from_input(env: &Env, input: Input) -> Result<Content> {
        match input {
            Input::Path(path_str) => Content::from_path(Path::new(&path_str)),
            Input::Url(url_str) => {
                Content::from_url(env, Url::parse(&url_str).expect("Failed to parse url"))
            }
            Input::ZuulBuild(path_buf, url_str, per_project) => {
                let url = Url::parse(&url_str).expect("Failed to parse url");
                let manifest = std::fs::File::open(
                    path_buf.as_path().join("zuul-info").join("inventory.yaml"),
                )
                .context("Loading inventory.yaml")?;
                let inventory_obj =
                    serde_yaml::from_reader(manifest).context("Decoding inventory.yaml")?;
                Ok(Content::LocalZuulBuild(
                    path_buf,
                    Box::new(crate::zuul::from_inventory(
                        url,
                        inventory_obj,
                        per_project,
                    )?),
                ))
            }
        }
    }

    /// Create content from raw file path, usefule for testing.
    pub fn from_pathbuf(p: PathBuf) -> Content {
        Content::File(Source::from_pathbuf(p))
    }

    /// Discover the baselines for this Content.
    #[tracing::instrument(level = "debug", skip(env))]
    pub fn discover_baselines(&self, env: &Env) -> Result<Baselines> {
        (match self {
            Content::File(src) => match src {
                Source::Local(_, pathbuf) => {
                    Content::discover_baselines_from_path(env, pathbuf.as_path())
                }
                Source::Remote(_, _) => Err(anyhow::anyhow!(
                    "Can't find remmote baselines, they need to be provided"
                )),
            },
            Content::Directory(_) => Err(anyhow::anyhow!(
                "Can't discover directory baselines, they need to be provided",
            )),
            Content::Prow(build) => build.discover_prow_baselines(),
            Content::Zuul(build) => crate::zuul::discover_baselines(build, env),
            Content::LocalZuulBuild(_, build) => crate::zuul::discover_baselines(build, env),
        })
        .and_then(|baselines| match baselines.len() {
            0 => Err(anyhow::anyhow!("Empty discovered baselines")),
            _ => {
                tracing::info!("Found the following baselines: {:?}", baselines);
                Ok(baselines)
            }
        })
    }

    /// Get the sources of log lines for this Content.
    #[tracing::instrument(level = "debug", skip(env))]
    pub fn get_sources(&self, env: &Env) -> Result<Vec<Source>> {
        self.get_sources_iter(env)
            .filter(|source| source.as_ref().map(is_source_valid).unwrap_or(true))
            .collect::<Result<Vec<_>>>()
            .and_then(|sources| match sources.len() {
                0 => Err(anyhow::anyhow!(format!("Empty sources: {}", self))),
                _ => Ok(sources),
            })
    }

    pub fn get_sources_iter(&self, env: &Env) -> Box<dyn Iterator<Item = Result<Source>>> {
        match self {
            Content::File(src) => Box::new(file_iter(src)),
            Content::Directory(src) => match src {
                Source::Local(_, pathbuf) => Box::new(dir_iter(pathbuf.as_path())),
                Source::Remote(_, url) => Box::new(httpdir_iter(url)),
            },
            Content::Zuul(build) => Box::new(crate::zuul::sources_iter(build)),
            Content::Prow(build) => Box::new(build.sources_prow_iter(env)),
            Content::LocalZuulBuild(src, _) => Box::new(dir_iter(src.as_path())),
        }
    }

    pub fn group_sources(
        env: &Env,
        baselines: &[Content],
    ) -> Result<HashMap<IndexName, Vec<Source>>> {
        let mut groups = HashMap::new();
        for baseline in baselines {
            for source in baseline.get_sources(env)? {
                groups
                    .entry(indexname_from_source(&source))
                    .or_insert_with(Vec::new)
                    .push(source);
            }
        }
        Ok(groups)
    }
}

impl Model {
    /// Create a Model from baselines.
    #[tracing::instrument(level = "debug", skip(mk_index, env))]
    pub fn train(env: &Env, baselines: Baselines, mk_index: fn() -> ChunkIndex) -> Result<Model> {
        let created_at = SystemTime::now();
        let mut indexes = HashMap::new();
        for (index_name, sources) in Content::group_sources(env, &baselines)?.drain() {
            env.debug_or_progress(&format!(
                "Loading index {} with {}",
                index_name,
                sources.iter().format(", ")
            ));
            let index = Index::train(env, &sources, mk_index())?;
            indexes.insert(index_name, index);
        }
        Ok(Model {
            created_at,
            baselines,
            indexes,
        })
    }

    fn validate_magic<R: Read>(input: &mut R) -> Result<()> {
        bincode::deserialize_from(input)
            .context("Loading model magic")
            .and_then(|cookie: String| {
                if cookie == MODEL_MAGIC {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("bad cookie: {}", cookie))
                }
            })
    }

    fn validate_version<R: Read>(input: &mut R) -> Result<()> {
        bincode::deserialize_from(input)
            .context("Loading model version")
            .and_then(|version: usize| {
                if version == MODEL_VERSION {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("bad version: {}", version))
                }
            })
    }

    fn validate_timestamp<R: Read>(input: &mut R) -> Result<SystemTime> {
        bincode::deserialize_from(input).context("Loading model timestamp")
    }

    fn validate<R: Read>(input: &mut R) -> Result<SystemTime> {
        Model::validate_magic(input)?;
        Model::validate_version(input)?;
        Model::validate_timestamp(input)
    }

    pub fn check(path: &Path) -> Result<SystemTime> {
        let mut input =
            flate2::read::GzDecoder::new(std::fs::File::open(path).context("Can't open file")?);
        Model::validate(&mut input)
    }

    pub fn load(path: &Path) -> Result<Model> {
        tracing::info!(path = path.to_str(), "Loading provided model");
        let mut input =
            flate2::read::GzDecoder::new(std::fs::File::open(path).context("Can't open file")?);
        Model::validate(&mut input)?;
        bincode::deserialize_from(input).context("Can't load model")
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        tracing::info!(path = path.to_str(), "Saving model");
        let mut output = flate2::write::GzEncoder::new(
            std::fs::File::create(path).context("Can't create file")?,
            flate2::Compression::fast(),
        );
        bincode::serialize_into(&mut output, MODEL_MAGIC).context("Can't save cookie")?;
        bincode::serialize_into(&mut output, &MODEL_VERSION).context("Can't save time")?;
        bincode::serialize_into(&mut output, &SystemTime::now()).context("Can't save time")?;
        bincode::serialize_into(output, self).context("Can't save model")
    }

    /// Get the matching index for a given Source.
    pub fn get_index<'a>(&'a self, index_name: &IndexName) -> Option<&'a Index> {
        lookup_or_single(&self.indexes, index_name)
    }

    /// Create the final report.
    #[tracing::instrument(level = "debug", skip(env, self))]
    pub fn report(&self, env: &Env, target: Content) -> Result<Report> {
        let start_time = Instant::now();
        let created_at = SystemTime::now();
        let mut index_reports = HashMap::new();
        let mut log_reports = Vec::new();
        let mut index_errors = Vec::new();
        let mut read_errors = Vec::new();
        let mut total_line_count = 0;
        let mut total_anomaly_count = 0;
        for (index_name, sources) in Content::group_sources(env, &[target.clone()])?.drain() {
            let mut skip_lines = KnownLines::new();
            match self.get_index(&index_name) {
                Some(index) => {
                    for source in sources {
                        let start_time = Instant::now();
                        let mut anomalies = Vec::new();
                        match index.get_processor(env, &source, &mut skip_lines) {
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
                                        index_reports.insert(index_name.clone(), index.to_report());
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
            target: format!("{}", target),
            baselines: self
                .baselines
                .iter()
                .map(|source| format!("{}", source))
                .collect(),
            log_reports,
            index_reports,
            index_errors,
            read_errors,
            total_line_count,
            total_anomaly_count,
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

#[test]
fn test_save_load() {
    let model = Model {
        created_at: SystemTime::now(),
        baselines: Vec::new(),
        indexes: HashMap::new(),
    };
    let dir = tempfile::tempdir().expect("tmpdir");
    let model_path = dir.path().join("model.bin");
    model.save(&model_path).expect("save");
    Model::load(&model_path).expect("load");
}
