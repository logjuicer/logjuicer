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

pub use logreduce_report::{
    AnomalyContext, ApiUrl, Content, IndexReport, LogReport, ProwBuild, Report, Source, ZuulBuild,
};

use crate::env::Env;
use crate::files::{dir_iter, file_iter, file_open};
use crate::unordered::KnownLines;
use crate::urls::{httpdir_iter, url_open};
pub mod config;
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
const MODEL_VERSION: usize = 6;

/// The user input.
#[derive(Debug, Serialize, Deserialize)]
pub enum Input {
    Path(String),
    Url(String),
    ZuulBuild(PathBuf, String),
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

    pub fn samples_count(&self) -> usize {
        self.index.samples_count()
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
        let is_job_output = if let Some((_, file_name)) = source.as_str().rsplit_once('/') {
            file_name.starts_with("job-output")
        } else {
            false
        };
        Ok(process::ChunkProcessor::new(
            fp,
            &self.index,
            source.is_json(),
            is_job_output,
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

/// Apply convertion rules to convert the user Input to Content.
#[tracing::instrument(level = "debug", skip(env))]
pub fn content_from_input(env: &Env, input: Input) -> Result<Content> {
    match input {
        Input::Path(path_str) => crate::files::content_from_path(Path::new(&path_str)),
        Input::Url(url_str) => {
            crate::urls::content_from_url(env, Url::parse(&url_str).expect("Failed to parse url"))
        }

        Input::ZuulBuild(path_buf, url_str) => {
            let url = ApiUrl::parse(&url_str).expect("Failed to parse url");
            let manifest =
                std::fs::File::open(path_buf.as_path().join("zuul-info").join("inventory.yaml"))
                    .context("Loading inventory.yaml")?;
            let inventory_obj =
                serde_yaml::from_reader(manifest).context("Decoding inventory.yaml")?;
            Ok(Content::LocalZuulBuild(
                path_buf,
                Box::new(crate::zuul::from_inventory(url, inventory_obj)?),
            ))
        }
    }
}

/// Create content from raw file path, usefule for testing.
pub fn content_from_pathbuf(p: PathBuf) -> Content {
    Content::File(Source::from_pathbuf(p))
}

/// Discover the baselines for this Content.
#[tracing::instrument(level = "debug", skip(env))]
pub fn content_discover_baselines(content: &Content, env: &Env) -> Result<Baselines> {
    (match content {
        Content::File(src) => match src {
            Source::Local(_, pathbuf) => {
                crate::files::discover_baselines_from_path(env, pathbuf.as_path())
            }
            Source::Remote(_, _) => Err(anyhow::anyhow!(
                "Can't find remmote baselines, they need to be provided"
            )),
        },
        Content::Directory(_) => Err(anyhow::anyhow!(
            "Can't discover directory baselines, they need to be provided",
        )),
        Content::Prow(build) => crate::prow::discover_baselines(build, env),
        Content::Zuul(build) => crate::zuul::discover_baselines(build, env),
        Content::LocalZuulBuild(_, build) => crate::zuul::discover_baselines(build, env),
    })
    .and_then(|baselines| match baselines.len() {
        0 => Err(anyhow::anyhow!("Empty discovered baselines")),
        _ => {
            tracing::info!(
                "Found the following baselines: {}",
                baselines.iter().format(", ")
            );
            Ok(baselines)
        }
    })
}

/// Get the sources of log lines for this Content.
#[tracing::instrument(level = "debug", skip(env))]
pub fn content_get_sources(content: &Content, env: &Env) -> Result<Vec<Source>> {
    content_get_sources_iter(content, env)
        .filter(|source| {
            source
                .as_ref()
                .map(|src| env.config.is_source_valid(src))
                .unwrap_or(true)
        })
        // FIXME: extract errors and emit them separately to avoid abort on a single error
        .collect::<Result<Vec<_>>>()
        .and_then(|sources| match sources.len() {
            0 => Err(anyhow::anyhow!(format!("Empty sources: {}", content))),
            _ => Ok(sources),
        })
}

pub fn content_get_sources_iter(
    content: &Content,
    env: &Env,
) -> Box<dyn Iterator<Item = Result<Source>>> {
    match content {
        Content::File(src) => Box::new(file_iter(src)),
        Content::Directory(src) => match src {
            Source::Local(_, pathbuf) => Box::new(dir_iter(pathbuf.as_path())),
            Source::Remote(_, url) => Box::new(httpdir_iter(url, env)),
        },
        Content::Zuul(build) => Box::new(crate::zuul::sources_iter(build, env)),
        Content::Prow(build) => Box::new(crate::prow::sources_iter(build, env)),
        Content::LocalZuulBuild(src, _) => Box::new(dir_iter(src.as_path())),
    }
}

pub fn group_sources(env: &Env, baselines: &[Content]) -> Result<HashMap<IndexName, Vec<Source>>> {
    let mut groups = HashMap::new();
    for baseline in baselines {
        for source in content_get_sources(baseline, env)? {
            groups
                .entry(indexname_from_source(&source))
                .or_insert_with(Vec::new)
                .push(source);
        }
    }
    Ok(groups)
}

impl Model {
    /// Create a Model from baselines.
    #[tracing::instrument(level = "debug", skip(mk_index, env))]
    pub fn train(env: &Env, baselines: Baselines, mk_index: fn() -> ChunkIndex) -> Result<Model> {
        let created_at = SystemTime::now();
        let mut indexes = HashMap::new();
        for (index_name, sources) in group_sources(env, &baselines)?.drain() {
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
        let mut unknown_files = HashMap::new();
        let mut read_errors = Vec::new();
        let mut total_line_count = 0;
        let mut total_anomaly_count = 0;
        for (index_name, sources) in group_sources(env, &[target.clone()])?.drain() {
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
                                            read_errors
                                                .push((source.clone(), format!("{}", err).into()));
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
                                read_errors.push((source.clone(), format!("{}", err).into()));
                                break;
                            }
                        }
                    }
                }
                None => {
                    let _ = unknown_files.insert(index_name, sources);
                }
            }
        }
        Ok(Report {
            created_at,
            run_time: start_time.elapsed(),
            target,
            baselines: self.baselines.clone(),
            log_reports: LogReport::sorted(log_reports),
            index_reports,
            unknown_files,
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

    fn samples_count(&self) -> usize {
        match self {
            ChunkIndex::HashingTrick(i) => i.samples_count(),
            ChunkIndex::Noop => 0,
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
        pub fn samples_count(&self) -> usize {
            self.baselines.iter().fold(0, |acc, fm| acc + fm.rows())
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
