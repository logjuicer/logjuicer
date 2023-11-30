// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a model implementation for the [logjuicer](https://github.com/logjuicer/logjuicer) project.
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

pub use logjuicer_tokenizer::index_name::IndexName;

pub use logjuicer_report::{
    AnomalyContext, ApiUrl, Content, IndexReport, LogReport, ProwBuild, Report, Source, ZuulBuild,
};

pub use logjuicer_index::{FeaturesMatrix, FeaturesMatrixBuilder};

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

use logjuicer_index::traits::*;

const MODEL_MAGIC: &str = "LGRD";

// Remember to bump this value when changing the tokenizer or the vectorizer to avoid using incompatible models.
const MODEL_VERSION: usize = 7;

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
pub struct Model<IR: IndexReader> {
    pub created_at: SystemTime,
    pub baselines: Baselines,
    pub indexes: HashMap<IndexName, Index<IR>>,
}

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
}

impl<IR: IndexReader> Index<IR> {
    #[tracing::instrument(level = "debug", name = "Index::train", skip(env, builder))]
    pub fn train<IB>(env: &Env, builder: IB, sources: &[Source]) -> Result<Index<IR>>
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
                Source::Local(_, path_buf) => file_open(path_buf.as_path())?,
                Source::Remote(prefix, url) => url_open(env, *prefix, url)?,
            };
            if let Err(e) = trainer.add(reader) {
                tracing::error!("{}: failed to load: {}", source, e)
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
        env: &Env,
        source: &Source,
        skip_lines: &'a mut KnownLines,
    ) -> Result<process::ChunkProcessor<IR, crate::reader::DecompressReader>> {
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
pub fn content_discover_baselines(content: &Content, env: &Env) -> Result<Baselines> {
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
        env: &Env,
        baselines: Baselines,
    ) -> Result<Model<IR>> {
        let created_at = SystemTime::now();
        let mut indexes = HashMap::new();
        for (index_name, sources) in group_sources(env, &baselines)?.drain() {
            env.debug_or_progress(&format!(
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
    #[tracing::instrument(level = "debug", skip(env, self, index, skip_lines))]
    pub fn report_source(
        &self,
        env: &Env,
        index: &Index<IR>,
        index_name: &IndexName,
        counters: &mut LineCounters,
        skip_lines: &mut KnownLines,
        source: &Source,
    ) -> std::result::Result<Option<LogReport>, String> {
        let start_time = Instant::now();
        let mut anomalies = Vec::new();
        match index.get_processor(env, source, skip_lines) {
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
                        index_name: index_name.clone(),
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
    pub fn report(&self, env: &Env, target: Content) -> Result<Report> {
        let start_time = Instant::now();
        let created_at = SystemTime::now();
        let mut index_reports = HashMap::new();
        let mut log_reports = Vec::new();
        let mut unknown_files = HashMap::new();
        let mut read_errors = Vec::new();
        let mut counters = LineCounters::new();
        for (index_name, sources) in group_sources(env, &[target.clone()])?.drain() {
            let mut skip_lines = KnownLines::new();
            match self.get_index(&index_name) {
                Some(index) => {
                    env.debug_or_progress(&format!(
                        "Reporting index {} with {}",
                        index_name,
                        sources.iter().take(5).format(", ")
                    ));
                    for source in sources {
                        match self.report_source(
                            env,
                            index,
                            &index_name,
                            &mut counters,
                            &mut skip_lines,
                            &source,
                        ) {
                            Ok(Some(lr)) => {
                                if !index_reports.contains_key(&index_name) {
                                    index_reports.insert(index_name.clone(), index.to_report());
                                };
                                log_reports.push(lr)
                            }
                            Ok(None) => {}
                            Err(err) => {
                                read_errors.push((source.clone(), err.into()));
                            }
                        }
                    }
                    tracing::debug!(skip_lines = skip_lines.len(), "reported one source");
                }
                None => {
                    env.debug_or_progress(&format!(
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
