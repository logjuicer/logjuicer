// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a model implementation for the [logjuicer](https://github.com/logjuicer/logjuicer) project.
//!
//! This module dispatch the abstract Content and Source to their implementationm e.g. the files module.

use anyhow::{Context, Result};
use env::{Env, TargetEnv};
use itertools::Itertools;
use logjuicer_report::Epoch;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use url::Url;

pub use logjuicer_tokenizer::index_name::IndexName;

pub use logjuicer_report::{
    AnomalyContext, ApiUrl, Content, IndexReport, LogReport, ProwBuild, Report, Source, ZuulBuild,
};

pub use logjuicer_index::{FeaturesMatrix, FeaturesMatrixBuilder};

use crate::files::{dir_iter, file_iter, file_open};
use crate::unordered::KnownLines;
use crate::urls::{httpdir_iter, url_open};
pub mod config;
pub mod env;
pub mod files;
pub mod process;
pub mod prow;
mod reader;
pub mod similarity;
pub mod timestamps;
pub mod unordered;
pub mod urls;
pub mod zuul;

use logjuicer_index::traits::*;

const MODEL_MAGIC: &str = "LGRD";

// Remember to bump this value when changing the tokenizer or the vectorizer to avoid using incompatible models.
pub const MODEL_VERSION: usize = 7;

/// The user input.
#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// An archive of baselines that is used to search anomaly.
#[derive(Debug, Serialize, Deserialize)]
pub struct Model<IR: IndexReader> {
    pub created_at: SystemTime,
    pub baselines: Vec<Content>,
    pub indexes: HashMap<IndexName, Index<IR>>,
}

/// The default model type
pub type ModelF = Model<FeaturesMatrix>;

pub fn indexname_from_source(source: &Source) -> IndexName {
    IndexName::from_path(source.get_relative())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index<IR: IndexReader> {
    pub created_at: SystemTime,
    pub train_time: Duration,
    pub sources: Vec<Source>,
    index: IR,
    pub line_count: usize,
    pub byte_count: usize,
}

impl<IR: IndexReader> Index<IR> {
    pub fn to_report(&self) -> IndexReport {
        IndexReport {
            train_time: self.train_time,
            sources: self.sources.clone(),
        }
    }

    pub fn samples_count(&self) -> usize {
        self.index.rows()
    }

    pub fn mappend(&self, other: &Index<IR>) -> Index<IR> {
        Index {
            created_at: self.created_at.max(other.created_at),
            train_time: self.train_time + other.train_time,
            sources: [&self.sources[..], &other.sources[..]].concat(),
            index: self.index.mappend(&other.index),
            line_count: self.line_count + other.line_count,
            byte_count: self.byte_count + other.byte_count,
        }
    }

    pub fn mconcat(&self, indexes: &[Index<IR>]) -> Index<IR> {
        Index {
            created_at: indexes
                .iter()
                .map(|i| i.created_at)
                .fold(self.created_at, |a, b| a.max(b)),
            train_time: indexes
                .iter()
                .map(|i| i.train_time)
                .fold(self.train_time, |a, b| a + b),
            sources: std::iter::once(self.sources.clone())
                .chain(indexes.iter().map(|i| i.sources.clone()))
                .flatten()
                .collect(),
            index: self
                .index
                .mconcat(&indexes.iter().map(|i| &i.index).collect::<Vec<_>>()),
            line_count: indexes
                .iter()
                .map(|i| i.line_count)
                .fold(self.line_count, |a, b| a + b),
            byte_count: indexes
                .iter()
                .map(|i| i.byte_count)
                .fold(self.byte_count, |a, b| a + b),
        }
    }
}

impl<IR: IndexReader> Model<IR> {
    /// Combine two models
    pub fn mappend(mut self, other: Model<IR>) -> Model<IR> {
        // Merge the other's indexes
        for (index_name, index) in other.indexes {
            let value = match self.indexes.get(&index_name) {
                // This is a new index, use it directly
                None => index,
                // This is an index update, combine it with the previous one
                Some(prev) => prev.mappend(&index),
            };
            self.indexes.insert(index_name, value);
        }

        // Merge the baselines
        self.baselines.extend(other.baselines);

        Model {
            created_at: self.created_at.max(other.created_at),
            baselines: self.baselines,
            indexes: self.indexes,
        }
    }

    /// Combine multiple models
    pub fn mconcat(mut self, others: Vec<Model<IR>>) -> Model<IR> {
        let mut created_at = self.created_at;

        // Collect the indexes so that they can be merged at once
        let mut other_indexes = HashMap::new();

        for other in others.into_iter() {
            self.baselines.extend(other.baselines);
            created_at = created_at.max(other.created_at);
            for (k, v) in other.indexes {
                other_indexes.entry(k).or_insert_with(Vec::new).push(v)
            }
        }

        for (index_name, mut indexes) in other_indexes {
            let base_index = match self.indexes.remove(&index_name) {
                None => indexes.pop().unwrap(),
                Some(index) => index,
            };
            let value = if indexes.is_empty() {
                base_index
            } else if indexes.len() == 1 {
                base_index.mappend(&indexes.pop().unwrap())
            } else {
                base_index.mconcat(&indexes)
            };
            self.indexes.insert(index_name, value);
        }
        Model {
            created_at,
            baselines: self.baselines,
            indexes: self.indexes,
        }
    }
}

impl<IR: IndexReader> Index<IR> {
    #[tracing::instrument(level = "debug", name = "Index::train", skip(env, builder))]
    pub fn train<IB>(env: &TargetEnv, builder: IB, sources: &[Source]) -> Result<Index<IR>>
    where
        IB: IndexBuilder<Reader = IR>,
    {
        let created_at = SystemTime::now();
        let start_time = Instant::now();
        let is_json = if let Some(source) = sources.first() {
            source.is_json()
        } else {
            false
        };
        let mut trainer = process::IndexTrainer::new(builder, is_json);
        for source in sources {
            let reader = match source {
                Source::Local(_, path_buf) => file_open(path_buf.as_path()),
                Source::Remote(prefix, url) => url_open(env.gl, *prefix, url),
            };
            // TODO: record training errors?
            match reader {
                Ok(reader) => {
                    if let Err(e) = trainer.add(env.config, reader) {
                        tracing::error!("{}: failed to load: {}", source, e)
                    }
                }
                Err(e) => tracing::error!("{}: failed to read {}", source, e),
            }
        }
        let line_count = trainer.line_count;
        let byte_count = trainer.byte_count;
        let index = trainer.build();
        let sources = sources.to_vec();
        let train_time = start_time.elapsed();
        Ok(Index {
            created_at,
            index,
            sources,
            train_time,
            line_count,
            byte_count,
        })
    }

    pub fn get_processor<'a>(
        &'a self,
        env: &'a TargetEnv,
        source: &Source,
        skip_lines: &'a mut Option<KnownLines>,
        gl_date: Option<Epoch>,
    ) -> Result<process::ChunkProcessor<IR, crate::reader::DecompressReader>> {
        let fp = match source {
            Source::Local(_, path_buf) => file_open(path_buf.as_path()),
            Source::Remote(prefix, url) => url_open(env.gl, *prefix, url),
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
            env.config,
            gl_date,
        ))
    }

    #[tracing::instrument(level = "debug", name = "Index::inspect", skip_all, fields(source))]
    pub fn inspect<'a>(
        &'a self,
        env: &'a TargetEnv,
        source: &Source,
        skip_lines: &'a mut Option<KnownLines>,
        gl_date: Option<Epoch>,
    ) -> Box<dyn Iterator<Item = Result<AnomalyContext>> + 'a> {
        match self.get_processor(env, source, skip_lines, gl_date) {
            Ok(processor) => Box::new(processor),
            // If the file can't be open, the first iterator result will be the error.
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }
}

/// Apply convertion rules to convert the user Input to Content.
#[tracing::instrument(level = "debug", skip(env), ret)]
pub fn content_from_input(env: &Env, input: Input) -> Result<Content> {
    match input {
        Input::Path(path_str) => crate::files::content_from_path(Path::new(&path_str)),
        Input::Url(url_str) => crate::urls::content_from_url(env, Url::parse(&url_str)?),

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
pub fn content_discover_baselines(content: &Content, env: &Env) -> Result<Vec<Content>> {
    (match content {
        Content::File(src) => match src {
            Source::Local(_, pathbuf) => {
                crate::files::discover_baselines_from_path(env, pathbuf.as_path())
            }
            Source::Remote(_, _) => Err(anyhow::anyhow!(
                "Use the diff command to process remote file.",
            )),
        },
        Content::Directory(_) => Err(anyhow::anyhow!(
            "Use the diff command to process directory.",
        )),
        Content::Prow(build) => crate::prow::discover_baselines(build, env),
        Content::Zuul(build) => crate::zuul::discover_baselines(build, env),
        Content::LocalZuulBuild(_, build) => crate::zuul::discover_baselines(build, env),
    })
    .and_then(|baselines| match baselines.len() {
        0 => Err(anyhow::anyhow!(
            "Baselines discovery failed, use the diff command to provide the baseline."
        )),
        _ => {
            tracing::info!(
                "Found the following baselines: {}.",
                baselines.iter().format(", ")
            );
            Ok(baselines)
        }
    })
}

/// Get the sources of log lines for this Content.
#[tracing::instrument(level = "debug", skip(env))]
pub fn content_get_sources(env: &TargetEnv, content: &Content) -> Result<Vec<Source>> {
    content_get_sources_iter(content, env.gl)
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

pub fn group_sources(
    env: &TargetEnv,
    baselines: &[Content],
) -> Result<HashMap<IndexName, Vec<Source>>> {
    let mut groups = HashMap::new();
    for baseline in baselines {
        for source in content_get_sources(env, baseline)? {
            groups
                .entry(indexname_from_source(&source))
                .or_insert_with(Vec::new)
                .push(source);
        }
    }
    Ok(groups)
}

#[derive(Debug)]
struct LineCounters {
    line_count: usize,
    anomaly_count: usize,
}

impl Default for LineCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl LineCounters {
    fn new() -> Self {
        LineCounters {
            line_count: 0,
            anomaly_count: 0,
        }
    }
}

impl<IR: IndexReader> Model<IR> {
    /// Create a Model from baselines.
    #[tracing::instrument(level = "debug", skip(env))]
    pub fn train<IB: Default + IndexBuilder<Reader = IR>>(
        env: &TargetEnv,
        mut baselines: Vec<Content>,
    ) -> Result<Model<IR>> {
        let created_at = SystemTime::now();
        let mut indexes = HashMap::new();

        // add extra baselines
        for baseline in &env.config.extra_baselines {
            baselines.push(baseline.clone());
        }

        for (index_name, sources) in group_sources(env, &baselines)?.drain() {
            env.gl.debug_or_progress(&format!(
                "Loading index {} with {}",
                index_name,
                sources.iter().format(", ")
            ));
            let builder = IB::default();
            let index = Index::train(env, builder, &sources)?;
            indexes.insert(index_name, index);
        }
        Ok(Model {
            created_at,
            baselines,
            indexes,
        })
    }

    /// Get the matching index for a given Source.
    pub fn get_index<'a>(&'a self, index_name: &IndexName) -> Option<&'a Index<IR>> {
        lookup_or_single(&self.indexes, index_name)
    }

    /// Create an individual LogReport.
    #[tracing::instrument(level = "debug", skip_all, fields(source))]
    pub fn report_source(
        &self,
        env: &TargetEnv,
        index: (&Index<IR>, &IndexName),
        counters: &mut LineCounters,
        skip_lines: &mut Option<KnownLines>,
        source: &Source,
        gl_date: Option<Epoch>,
    ) -> std::result::Result<Option<LogReport>, String> {
        let start_time = Instant::now();
        let mut anomalies = Vec::new();
        match index.0.get_processor(env, source, skip_lines, gl_date) {
            Ok(mut processor) => {
                for anomaly in processor.by_ref() {
                    match anomaly {
                        Ok(anomaly) => anomalies.push(anomaly),
                        Err(err) => return Err(format!("{}", err)),
                    }
                }
                counters.line_count += processor.line_count;
                if !anomalies.is_empty() {
                    counters.anomaly_count += anomalies.len();

                    Ok(Some(LogReport {
                        test_time: start_time.elapsed(),
                        anomalies,
                        source: source.clone(),
                        index_name: index.1.clone(),
                        line_count: processor.line_count,
                        byte_count: processor.byte_count,
                    }))
                } else {
                    Ok(None)
                }
            }
            Err(err) => Err(format!("{}", err)),
        }
    }

    /// Create the final report.
    #[tracing::instrument(level = "debug", skip(env, self))]
    pub fn report(&self, env: &TargetEnv, target: Content) -> Result<Report> {
        let start_time = Instant::now();
        let created_at = SystemTime::now();
        let mut index_reports = HashMap::new();
        let mut log_reports = Vec::new();
        let mut unknown_files = HashMap::new();
        let mut read_errors = Vec::new();
        let mut counters = LineCounters::new();
        let mut gl_date = None;
        for (index_name, sources) in group_sources(env, &[target.clone()])?.drain() {
            let mut skip_lines = env.new_skip_lines();
            match self.get_index(&index_name) {
                Some(index) => {
                    env.gl.debug_or_progress(&format!(
                        "Reporting index {} with {}",
                        index_name,
                        sources.iter().take(5).format(", ")
                    ));
                    for source in sources {
                        match self.report_source(
                            env,
                            (index, &index_name),
                            &mut counters,
                            &mut skip_lines,
                            &source,
                            gl_date,
                        ) {
                            Ok(Some(lr)) => {
                                if !index_reports.contains_key(&index_name) {
                                    index_reports.insert(index_name.clone(), index.to_report());
                                };
                                if gl_date.is_none() {
                                    // Record the global date if it is unknown
                                    gl_date = lr.timed().next().map(|ea| ea.0)
                                };
                                log_reports.push(lr)
                            }
                            Ok(None) => {}
                            Err(err) => {
                                read_errors.push((source.clone(), err.into()));
                            }
                        }
                    }
                    tracing::debug!(
                        skip_lines = skip_lines.as_ref().map_or(0, |s| s.len()),
                        "reported one source"
                    );
                }
                None => {
                    env.gl.debug_or_progress(&format!(
                        "Unknown index index {} for {} sources",
                        index_name,
                        sources.len()
                    ));
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
            total_line_count: counters.line_count,
            total_anomaly_count: counters.anomaly_count,
        })
    }
}

impl<IR: IndexReader + Serialize + serde::de::DeserializeOwned> Model<IR> {
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
        Model::<IR>::validate_magic(input)?;
        Model::<IR>::validate_version(input)?;
        Model::<IR>::validate_timestamp(input)
    }

    pub fn check(path: &Path) -> Result<SystemTime> {
        let mut input =
            flate2::read::GzDecoder::new(std::fs::File::open(path).context("Can't open file")?);
        Model::<IR>::validate(&mut input)
    }

    pub fn load(path: &Path) -> Result<Model<IR>> {
        tracing::info!(path = path.to_str(), "Loading provided model");
        let mut input =
            flate2::read::GzDecoder::new(std::fs::File::open(path).context("Can't open file")?);
        Model::<IR>::validate(&mut input)?;
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
}

/// Helper function to make a single value hash map always match the key.
/// This is useful when logjuicer is used to compare two files which may have different index name.
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

#[test]
fn test_save_load() {
    let model: Model<logjuicer_index::FeaturesMatrix> = Model {
        created_at: SystemTime::now(),
        baselines: Vec::new(),
        indexes: HashMap::new(),
    };
    let dir = tempfile::tempdir().expect("tmpdir");
    let model_path = dir.path().join("model.bin");
    model.save(&model_path).expect("save");
    Model::<logjuicer_index::FeaturesMatrix>::load(&model_path).expect("load");
}
