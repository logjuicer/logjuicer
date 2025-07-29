// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides a model implementation for the [logjuicer](https://github.com/logjuicer/logjuicer) project.
//!
//! This module dispatch the abstract Content and Source to their implementationm e.g. the files module.

use anyhow::{Context, Result};
use env::{Env, TargetEnv};
use itertools::Itertools;
use rayon::prelude::*;
use reader::DecompressReader;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use url::Url;

pub use logjuicer_index::{FeaturesMatrix, FeaturesMatrixBuilder};
use logjuicer_report::Epoch;
pub use logjuicer_report::{
    AnomalyContext, ApiUrl, Content, IndexReport, LogReport, ProwBuild, Report, Source, SourceLoc,
    ZuulBuild,
};
pub use logjuicer_tokenizer::index_name::IndexName;

use crate::files::{dir_iter, file_iter};
use crate::unordered::KnownLines;
use crate::urls::{httpdir_iter, url_open};
pub mod config;
pub mod env;
pub mod errors;
pub mod files;
pub mod journal;
pub mod process;
pub mod prow;
mod reader;
pub mod similarity;
pub mod source;
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
    if let Some(tarball) = source.is_tarfile() {
        let tarball_index = IndexName::from_path(tarball.get_relative());
        let entry_index = IndexName::from_path(source.get_relative());
        tarball_index.extend(&entry_index)
    } else {
        IndexName::from_path(source.get_relative())
    }
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

#[allow(clippy::large_enum_variant)]
enum IndexSource<'a> {
    Bundle(Vec<Source>),
    Tarfile(Source, DecompressReader<'a>),
}

impl std::fmt::Debug for IndexSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexSource::Bundle(x) => x.fmt(f),
            IndexSource::Tarfile(x, _) => x.fmt(f),
        }
    }
}

impl<IR: IndexReader> Index<IR> {
    #[tracing::instrument(level = "debug", name = "Index::train", skip(env, builder))]
    pub fn train<'a, IB>(
        env: &TargetEnv,
        builder: IB,
        sources: IndexSource<'a>,
    ) -> Option<Index<IR>>
    where
        IB: IndexBuilder<Reader = IR>,
    {
        let created_at = SystemTime::now();
        let start_time = Instant::now();
        let mut trainer = process::IndexTrainer::new(builder);
        let mut train_source = |source, reader| match source::LinesIterator::new(source, reader) {
            Ok(reader) => {
                env.set_current(source);
                if let Err(e) = trainer.add(env.config, reader) {
                    tracing::error!("{}: failed to load: {}", source, e)
                }
            }
            Err(e) => tracing::error!("{}: failed to read {}", source, e),
        };
        let sources = match sources {
            IndexSource::Bundle(sources) => {
                for source in &sources {
                    match crate::source::open_single_source(env.gl, source) {
                        Ok(reader) => train_source(source, reader),
                        Err(e) => tracing::error!("{}: fail to open {}", source, e),
                    };
                }
                sources
            }
            IndexSource::Tarfile(source, reader) => {
                train_source(&source, reader);
                vec![source]
            }
        };
        let line_count = trainer.line_count;
        let byte_count = trainer.byte_count;
        let index = trainer.build();
        if index.rows() > 0 {
            let train_time = start_time.elapsed();
            Some(Index {
                created_at,
                index,
                sources,
                train_time,
                line_count,
                byte_count,
            })
        } else {
            None
        }
    }

    pub fn get_processor<'a, 'b>(
        &'a self,
        env: &'a TargetEnv,
        source: &Source,
        reader: DecompressReader<'b>,
        skip_lines: &'a mut Option<KnownLines>,
        gl_date: Option<Epoch>,
    ) -> Result<process::ChunkProcessor<'a, IR, crate::reader::DecompressReader<'b>>> {
        let reader = source::LinesIterator::new(source, reader)?;
        let is_job_output = if let Some((_, file_name)) = source.as_str().rsplit_once('/') {
            file_name.starts_with("job-output")
        } else {
            false
        };
        env.set_current(source);
        Ok(process::ChunkProcessor::new(
            reader,
            &self.index,
            is_job_output,
            skip_lines,
            env.config,
            gl_date,
        ))
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
    Content::File(SourceLoc::from_pathbuf(p))
}

/// Discover the baselines for this Content.
#[tracing::instrument(level = "debug", skip(env))]
pub fn content_discover_baselines(content: &Content, env: &Env) -> Result<Vec<Content>> {
    (match content {
        Content::File(src) => match src {
            SourceLoc::Local(_, pathbuf) => {
                crate::files::discover_baselines_from_path(env, pathbuf.as_path())
            }
            SourceLoc::Remote(_, _) => Err(anyhow::anyhow!(
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
pub fn content_get_sources(env: &TargetEnv, content: &Content) -> Result<Vec<SourceLoc>> {
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
) -> Box<dyn Iterator<Item = Result<SourceLoc>>> {
    match content {
        Content::File(src) => Box::new(file_iter(src)),
        Content::Directory(src) => match src {
            SourceLoc::Local(_, pathbuf) => Box::new(dir_iter(pathbuf.as_path())),
            SourceLoc::Remote(_, url) => Box::new(httpdir_iter(url, env)),
        },
        Content::Zuul(build) => Box::new(crate::zuul::sources_iter(build, env)),
        Content::Prow(build) => Box::new(crate::prow::sources_iter(build, env)),
        Content::LocalZuulBuild(src, _) => Box::new(dir_iter(src.as_path())),
    }
}

type GroupResult = (Vec<SourceLoc>, HashMap<IndexName, Vec<Source>>);

pub fn group_sources(env: &TargetEnv, baselines: &[Content]) -> Result<GroupResult> {
    let mut groups = HashMap::new();
    let mut tarballs = Vec::new();
    for baseline in baselines {
        for source in content_get_sources(env, baseline)? {
            if source.is_tarball() {
                tarballs.push(source);
            } else {
                groups
                    .entry(IndexName::from_path(source.get_relative()))
                    .or_insert_with(Vec::new)
                    .push(Source::RawFile(source));
            }
        }
    }
    Ok((tarballs, groups))
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

impl<IR: IndexReader + Send + Sync> Model<IR> {
    /// Create a Model from baselines.
    #[tracing::instrument(level = "debug", skip(env))]
    pub fn train<IB: Default + IndexBuilder<Reader = IR>>(
        env: &TargetEnv,
        baselines: Vec<Content>,
    ) -> Result<Model<IR>> {
        let created_at = SystemTime::now();
        let indexes = Mutex::new(HashMap::new());

        let (tarballs, mut groups) = group_sources(env, &baselines)?;

        groups
            .drain()
            .collect::<Vec<(IndexName, Vec<Source>)>>()
            .into_par_iter()
            .for_each(|(index_name, sources)| {
                env.gl.debug_or_progress(&format!(
                    "Loading index {} with {}",
                    index_name,
                    sources.iter().format(", ")
                ));
                let builder = IB::default();
                match Index::train(env, builder, IndexSource::Bundle(sources)) {
                    Some(index) => {
                        indexes.lock().unwrap().insert(index_name, index);
                    }
                    None => tracing::error!("{}: empty index", index_name),
                };
            });
        tarballs.into_par_iter().for_each(|tarball| {
            crate::source::with_source(env, tarball, |source, reader| {
                let builder = IB::default();
                let index_name = indexname_from_source(&source);
                match reader.and_then(|reader| {
                    Index::train(env, builder, IndexSource::Tarfile(source.clone(), reader))
                        .ok_or_else(|| "empty index".to_string())
                }) {
                    Ok(index) => {
                        let mut indexes = indexes.lock().unwrap();
                        let index = if let Some(prev_index) = indexes.get(&index_name) {
                            prev_index.mappend(&index)
                        } else {
                            index
                        };
                        indexes.insert(index_name, index);
                    }
                    Err(err) => tracing::error!("{}: {}", index_name, err),
                };
            })
        });
        let indexes = indexes.into_inner().unwrap();
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
    pub fn report_source<'b>(
        &self,
        env: &TargetEnv,
        index: (&Index<IR>, &IndexName),
        counters: Arc<Mutex<LineCounters>>,
        skip_lines: &mut Option<KnownLines>,
        source: (&Source, DecompressReader<'b>),
        gl_date: Option<Epoch>,
    ) -> std::result::Result<Option<LogReport>, String> {
        let start_time = Instant::now();
        let mut anomalies = Vec::new();
        match index
            .0
            .get_processor(env, source.0, source.1, skip_lines, gl_date)
        {
            Ok(mut processor) => {
                for anomaly in processor.by_ref() {
                    match anomaly {
                        Ok(anomaly) => anomalies.push(anomaly),
                        Err(err) => return Err(format!("{}", err)),
                    }
                }
                let mut counters = counters.lock().unwrap();
                counters.line_count += processor.line_count;
                if !anomalies.is_empty() {
                    counters.anomaly_count += anomalies.len();

                    Ok(Some(LogReport {
                        test_time: start_time.elapsed(),
                        anomalies,
                        source: source.0.clone(),
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
        let index_reports = Mutex::new(HashMap::new());
        let log_reports = Mutex::new(Vec::new());
        let unknown_files = Mutex::new(HashMap::new());
        let read_errors = Mutex::new(Vec::new());
        let counters = Arc::new(Mutex::new(LineCounters::new()));
        let gl_date = Mutex::new(None);
        let (tarballs, mut groups) = group_sources(env, &[target.clone()])?;
        for (index_name, sources) in groups.drain() {
            let mut skip_lines = env.new_skip_lines();
            match self.get_index(&index_name) {
                Some(index) => {
                    env.gl.debug_or_progress(&format!(
                        "Reporting index {} with {}",
                        index_name,
                        sources.iter().take(5).format(", ")
                    ));
                    for source in sources {
                        let reader = crate::source::open_single_source(env.gl, &source)?;
                        let cur_date = *gl_date.lock().unwrap();
                        match self.report_source(
                            env,
                            (index, &index_name),
                            counters.clone(),
                            &mut skip_lines,
                            (&source, reader),
                            cur_date,
                        ) {
                            Ok(Some(lr)) => {
                                let mut index_reports = index_reports.lock().unwrap();
                                if !index_reports.contains_key(&index_name) {
                                    index_reports.insert(index_name.clone(), index.to_report());
                                };
                                let mut gl_date = gl_date.lock().unwrap();
                                if gl_date.is_none() {
                                    // Record the global date if it is unknown
                                    *gl_date = lr.timed().next().map(|ea| ea.0)
                                };
                                log_reports.lock().unwrap().push(lr)
                            }
                            Ok(None) => {}
                            Err(err) => {
                                read_errors
                                    .lock()
                                    .unwrap()
                                    .push((source.clone(), err.into()));
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
                    let _ = unknown_files.lock().unwrap().insert(index_name, sources);
                }
            }
        }
        tarballs.into_par_iter().for_each(|tarball| {
            crate::source::with_source(env, tarball, |source, reader| {
                let index_name = indexname_from_source(&source);
                match self.get_index(&index_name) {
                    Some(index) => {
                        env.gl.debug_or_progress(&format!(
                            "Reporting index {} with {}",
                            index_name,
                            source.get_relative()
                        ));
                        let mut skip_lines = env.new_skip_lines();
                        let cur_date = *gl_date.lock().unwrap();
                        match reader.and_then(|reader| {
                            self.report_source(
                                env,
                                (index, &index_name),
                                counters.clone(),
                                &mut skip_lines,
                                (&source, reader),
                                cur_date,
                            )
                        }) {
                            Ok(Some(lr)) => {
                                let mut index_reports = index_reports.lock().unwrap();
                                if !index_reports.contains_key(&index_name) {
                                    index_reports.insert(index_name.clone(), index.to_report());
                                };
                                let mut gl_date = gl_date.lock().unwrap();
                                if gl_date.is_none() {
                                    // Record the global date if it is unknown
                                    *gl_date = lr.timed().next().map(|ea| ea.0)
                                };
                                log_reports.lock().unwrap().push(lr)
                            }
                            Ok(None) => {}
                            Err(err) => {
                                read_errors
                                    .lock()
                                    .unwrap()
                                    .push((source.clone(), err.into()));
                            }
                        }
                    }
                    None => {
                        env.gl.debug_or_progress(&format!(
                            "Unknown index index {} for {} sources",
                            index_name,
                            source.get_relative()
                        ));
                        unknown_files
                            .lock()
                            .unwrap()
                            .entry(index_name)
                            .or_insert_with(Vec::new)
                            .push(source);
                    }
                }
            })
        });
        let unknown_files = unknown_files.into_inner().unwrap();
        let log_reports = log_reports.into_inner().unwrap();
        let index_reports = index_reports.into_inner().unwrap();
        let read_errors = read_errors.into_inner().unwrap();
        let counters = counters.lock().unwrap();
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
