// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use logjuicer_model::config::DiskSizeLimit;
use logjuicer_model::env::TargetEnv;
use logjuicer_model::similarity::create_similarity_report;
use logjuicer_model::ModelF;
use logjuicer_report::model_row::ContentID;
use logjuicer_report::report_row::FileSize;
use logjuicer_report::Content;
use logjuicer_report::SimilarityReport;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::RwLock;
use std::sync::{Arc, Mutex};

use logjuicer_model::env::{CurrentTarget, EnvConfig};
use logjuicer_report::report_row::{ReportID, ReportStatus};
use logjuicer_report::Report;

use crate::database::report_path;
use crate::database::Db;
use crate::routes::ReportRequest;

type ReportsLock = Arc<RwLock<BTreeMap<ReportID, ProcessMonitor>>>;
type ModelsLock = Arc<RwLock<BTreeMap<ContentID, ProcessMonitor>>>;

#[derive(Clone)]
pub struct Workers {
    /// Allow the worker to fetch any user input.
    allow_any_sources: bool,
    /// The execution pool to run logjuicer model.
    pool: threadpool::ThreadPool,
    /// The report process monitor to broadcast the status to websocket clients.
    reports: ReportsLock,
    /// The models being created
    models: ModelsLock,
    /// The logjuicer environment.
    env: Arc<EnvConfig>,
    /// The local database of reports.
    pub db: Db,
    pub storage_dir: Arc<str>,
    pub current_files_size: Arc<AtomicUsize>,
    size_limit: DiskSizeLimit,
}

const MAX_LOGJUICER_PROCESS: usize = 2;

impl Workers {
    pub async fn new(
        allow_any_sources: bool,
        storage_dir: Arc<str>,
        size_limit: DiskSizeLimit,
        env: EnvConfig,
    ) -> Self {
        let current_files_size = Arc::new(AtomicUsize::new(0));
        std::fs::create_dir_all(format!("{storage_dir}/models")).unwrap();
        // TODO: migrate reports to a reports sub directory
        let db = Db::new(&storage_dir, current_files_size.clone())
            .await
            .unwrap();
        Workers {
            allow_any_sources,
            db,
            pool: threadpool::ThreadPool::new(MAX_LOGJUICER_PROCESS),
            env: Arc::new(env),
            reports: Arc::new(RwLock::new(BTreeMap::new())),
            models: Arc::new(RwLock::new(BTreeMap::new())),
            storage_dir,
            current_files_size,
            size_limit,
        }
    }

    fn reclaim_any_space(&self) {
        if self
            .current_files_size
            .load(std::sync::atomic::Ordering::Relaxed)
            > self.size_limit.max
        {
            let db = self.db.clone();
            let storage_dir = self.storage_dir.clone();
            let disk_size_limit = self.size_limit.clone();
            tokio::runtime::Handle::current()
                .spawn(async move { db.reclaim_space(&storage_dir, disk_size_limit).await });
        }
    }

    pub fn subscribe(&self, report_id: ReportID) -> Option<ProcessFollower> {
        let running = self.reports.read().unwrap();
        running.get(&report_id).map(|pm| ProcessFollower {
            events: pm.events.clone(),
            chan: pm.chan.subscribe(),
            current: pm.current.clone(),
        })
    }

    #[cfg(test)]
    pub async fn wait(&self, report_id: ReportID) -> Vec<Arc<str>> {
        if let Some(mut monitor) = self.subscribe(report_id) {
            let mut events = monitor.events.read().await.clone();
            while let Ok(msg) = monitor.chan.recv().await {
                events.push(msg)
            }
            events
        } else {
            vec![]
        }
    }

    pub fn submit(&self, report_id: ReportID, report_request: ReportRequest) {
        if let Some(monitor) = report_lock(report_id, &self.reports) {
            tracing::info!("Submiting report request {}", report_request);

            // Cleanup if necessary
            self.reclaim_any_space();

            // Prepare worker variables
            let env = self.env.clone();
            let models_lock = self.models.clone();
            let reports = self.reports.clone();
            let db = self.db.clone();
            let handle = tokio::runtime::Handle::current();
            let storage_dir = self.storage_dir.clone();
            let allow_any_sources = self.allow_any_sources;

            // Submit the execution to the thread pool
            self.pool.execute(move || {
                let penv = ProcessEnv {
                    storage_dir,
                    monitor,
                    models_lock,
                    db: db.clone(),
                    handle: handle.clone(),
                    allow_any_sources,
                };
                let monitor = &penv.monitor;
                let (status, count, size) = match process_report_safe(&penv, &env, report_request) {
                    Ok(report) => {
                        let fp = report_path(&penv.storage_dir, report_id);
                        let path = std::path::Path::new(&fp);
                        let (count, save_result) = match report {
                            ReportResult::NewReport(report) => {
                                (report.anomaly_count(), report.save(path))
                            }
                            ReportResult::NewSimilarity(report) => {
                                ((report.similarity_reports.len()), report.save(path))
                            }
                        };
                        let path = std::path::Path::new(&fp);
                        let (status, size) = if let Err(err) = save_result {
                            tracing::error!("{}: failed to save report: {}", fp, err);
                            monitor.emit(format!("Error: saving failed: {}", err).into());
                            (
                                ReportStatus::Error(format!("Save error: {}", err)),
                                FileSize(0),
                            )
                        } else {
                            tracing::info!("{}: saved report", fp);
                            monitor.emit("Done".into());
                            (ReportStatus::Completed, FileSize::from(path))
                        };
                        (status, count, size)
                    }
                    Err(e) => {
                        monitor.emit(format!("Error: {}", e).into());
                        (ReportStatus::Error(e), 0, FileSize(0))
                    }
                };
                // Remove the monitor
                let _ = reports.write().unwrap().remove(&report_id);
                // Record the result into the db
                handle.spawn(async move {
                    db.update_report(report_id, count, &status, size)
                        .await
                        .unwrap()
                });
            })
        } else {
            tracing::info!("Url already submitted {}", report_request);
        }
    }
}

fn report_lock(report_id: ReportID, reports: &ReportsLock) -> Option<ProcessMonitor> {
    let mut reports_init_write = reports.write().unwrap();
    // Check if the report is being processed
    if let std::collections::btree_map::Entry::Vacant(e) = reports_init_write.entry(report_id) {
        let monitor = ProcessMonitor::new();
        e.insert(monitor.clone());
        Some(monitor)
    } else {
        None
    }
}

struct ProcessEnv {
    allow_any_sources: bool,
    storage_dir: Arc<str>,
    monitor: ProcessMonitor,
    db: Db,
    models_lock: ModelsLock,
    handle: tokio::runtime::Handle,
}

pub struct ProcessFollower {
    pub events: Arc<tokio::sync::RwLock<Vec<Arc<str>>>>,
    pub chan: tokio::sync::broadcast::Receiver<Arc<str>>,
    pub current: CurrentTarget,
}

#[derive(Clone)]
pub struct ProcessMonitor {
    pub events: Arc<tokio::sync::RwLock<Vec<Arc<str>>>>,
    pub chan: tokio::sync::broadcast::Sender<Arc<str>>,
    pub current: CurrentTarget,
}

impl ProcessMonitor {
    fn new() -> Self {
        let (chan, _) = tokio::sync::broadcast::channel(16);
        ProcessMonitor {
            events: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            chan,
            current: Arc::new(Mutex::new(None)),
        }
    }

    fn emit(&self, msg: Arc<str>) {
        // println!("Emitting {}", msg);
        self.events.blocking_write().push(msg.clone());
        let _ = self.chan.send(msg);
    }

    async fn emit_async(&self, msg: Arc<str>) {
        self.events.write().await.push(msg.clone());
        let _ = self.chan.send(msg);
    }
}

#[allow(clippy::large_enum_variant)]
enum ReportResult {
    NewReport(Report),
    NewSimilarity(SimilarityReport),
}

fn process_report_safe(
    penv: &ProcessEnv,
    env: &EnvConfig,
    report_request: ReportRequest,
) -> Result<ReportResult, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match report_request {
        ReportRequest::NewSimilarity(rids) => {
            process_similarity(penv, rids).map(ReportResult::NewSimilarity)
        }
        ReportRequest::NewReport(args) => match args.errors {
            Some(true) => process_errors_report(penv, env, &args.target),
            Some(false) | None => process_report(
                penv,
                env,
                &args.target,
                args.baseline.as_deref(),
                args.errors.is_none(),
            ),
        }
        .map(ReportResult::NewReport),
    })) {
        Ok(res) => res,
        Err(err) => Err(format!(
            "crashed {}",
            err.downcast::<&str>().unwrap_or(Box::new("unknown"))
        )),
    }
}

fn process_similarity(
    penv: &ProcessEnv,
    reports_id: Vec<ReportID>,
) -> Result<SimilarityReport, String> {
    let reports: Vec<Report> = reports_id
        .into_iter()
        .map(|rid| {
            let fp = report_path(&penv.storage_dir, rid);
            Report::load(Into::<PathBuf>::into(&fp).as_path())
                .map_err(|e| format!("{fp}: loading failed: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let reports: Vec<&Report> = reports.iter().collect();
    Ok(create_similarity_report(&reports))
}

fn process_errors_report(
    penv: &ProcessEnv,
    env: &EnvConfig,
    target: &str,
) -> Result<Report, String> {
    let monitor = &penv.monitor;
    monitor.emit(format!("Running `logjuicer errors {}`", target).into());
    let content = resolve_content(penv, env, target)?;
    do_process_errors_report(penv, env, content)
}

fn do_process_errors_report(
    penv: &ProcessEnv,
    env: &EnvConfig,
    content: Content,
) -> Result<Report, String> {
    let target_env = env.get_target_env_with_current(&content, Some(penv.monitor.current.clone()));
    logjuicer_model::errors::errors_report(&target_env, content)
        .map_err(|e| format!("report failed: {:?}", e))
}

fn check_content(content: &Content) -> Result<(), String> {
    match content {
        Content::Zuul(_) | logjuicer_report::Content::Prow(_) => Ok(()),
        _ => Err("Only zuul or prow build are supported".to_string()),
    }
}

fn resolve_content(penv: &ProcessEnv, env: &EnvConfig, target: &str) -> Result<Content, String> {
    let input = logjuicer_model::Input::Url(target.into());
    let content =
        logjuicer_model::content_from_input(&env.gl, input).map_err(|e| format!("{:?}", e))?;

    penv.monitor
        .emit(format!("Content resolved: {}", content).into());
    if !penv.allow_any_sources {
        check_content(&content)?;
    }
    Ok(content)
}

fn process_report(
    penv: &ProcessEnv,
    env: &EnvConfig,
    target: &str,
    baseline: Option<&str>,
    auto_error: bool,
) -> Result<Report, String> {
    let monitor = &penv.monitor;
    match baseline {
        None => monitor.emit(format!("Running `logjuicer url {}`", target).into()),
        Some(baseline) => {
            monitor.emit(format!("Running `logjuicer diff {} {}`", baseline, target).into())
        }
    }

    let content = resolve_content(penv, env, target)?;
    let baselines = match baseline {
        Some(baseline) => {
            let input = logjuicer_model::Input::Url(baseline.into());
            vec![logjuicer_model::content_from_input(&env.gl, input)
                .map_err(|e| format!("baseline: {:?}", e))?]
        }
        None => match logjuicer_model::content_discover_baselines(&content, &env.gl) {
            Ok(xs) => xs,
            Err(e) if auto_error => {
                monitor
                    .emit(format!("discovery failed: {:?}, performing an errors report", e).into());
                return do_process_errors_report(penv, env, content);
            }
            Err(e) => {
                return Err(format!("discovery failed: {:?}", e));
            }
        },
    };

    if baseline.is_none() {
        monitor.emit(format!("Baseline found: {}", baselines.iter().format(", ")).into());
    }
    if !penv.allow_any_sources {
        baselines.iter().try_for_each(check_content)?;
    }

    let target_env = env.get_target_env_with_current(&content, Some(monitor.current.clone()));
    let model: ModelF = process_models(penv, &target_env, baselines)?;

    monitor.emit("Starting analysis".into());
    let report = model
        .report(&target_env, content)
        .map_err(|e| format!("report failed: {:?}", e))?;
    target_env.clear_current();

    Ok(report)
}

fn process_models(
    penv: &ProcessEnv,
    target_env: &TargetEnv,
    baselines: Vec<Content>,
) -> Result<ModelF, String> {
    let baselines_iter = baselines
        .into_iter()
        .map(|content| process_model(penv, target_env, content));
    let extra_iter = target_env
        .config
        .extra_baselines
        .iter()
        .map(|content| process_model(penv, target_env, content.clone()));
    let mut models = baselines_iter
        .chain(extra_iter)
        .collect::<Result<Vec<ModelF>, String>>()?;
    if let Some(model) = models.pop() {
        Ok(if models.is_empty() {
            // There is only a single baseline, use the model directly
            model
        } else if models.len() == 1 {
            // There are two baselines, use the fast mappend operation
            model.mappend(models.pop().unwrap())
        } else {
            // There are more than two baselines, use the mconcat operation
            model.mconcat(models)
        })
    } else {
        Err("No models found".to_string())
    }
}

enum ModelStatus {
    Existing,
    Pending(ProcessFollower),
    ToBuild(ProcessMonitor),
}

fn model_lock(
    penv: &ProcessEnv,
    content: &Content,
    content_id: &ContentID,
) -> Result<ModelStatus, String> {
    let mut models_lock = penv.models_lock.write().unwrap();
    match models_lock.get(content_id) {
        // Someone is already building it
        Some(monitor) => Ok(ModelStatus::Pending(ProcessFollower {
            events: monitor.events.clone(),
            chan: monitor.chan.subscribe(),
            current: monitor.current.clone(),
        })),
        // Nobody is building it
        None => match penv
            .handle
            .block_on(penv.db.lookup_model(content_id))
            .map_err(|e| format!("db model lookup: {}", e))?
        {
            // The model was already built and the content did not change.
            Some(created_at) if content.older_than(created_at) => Ok(ModelStatus::Existing),
            // The model is not in the database, or the content is newer than the previous model.
            _ => {
                let monitor = ProcessMonitor::new();
                models_lock.insert(content_id.clone(), monitor.clone());
                Ok(ModelStatus::ToBuild(monitor))
            }
        },
    }
}

fn process_model(
    penv: &ProcessEnv,
    target_env: &TargetEnv,
    content: Content,
) -> Result<ModelF, String> {
    let content_id = (&content).into();
    match model_lock(penv, &content, &content_id)? {
        ModelStatus::Existing => {
            penv.monitor.emit("Loading existing model".into());
            crate::models::load_model(&penv.storage_dir, &content_id)
        }
        ModelStatus::Pending(mut model_follower) => {
            penv.handle.block_on(async {
                penv.monitor
                    .emit_async("Waiting for model build".into())
                    .await;
                // forward previous messages
                for msg in model_follower.events.read().await.iter() {
                    penv.monitor.emit_async(Arc::clone(msg)).await;
                }
                // forward current messages
                while let Ok(msg) = model_follower.chan.recv().await {
                    penv.monitor.emit_async(msg).await;
                }
            });
            crate::models::load_model(&penv.storage_dir, &content_id)
        }
        ModelStatus::ToBuild(model_monitor) => {
            let result = do_process_model(model_monitor, penv, target_env, content, &content_id);
            // Remove the monitor
            let _ = penv.models_lock.write().unwrap().remove(&content_id);
            result
        }
    }
}

fn do_process_model(
    model_monitor: ProcessMonitor,
    penv: &ProcessEnv,
    target_env: &TargetEnv,
    content: Content,
    content_id: &ContentID,
) -> Result<ModelF, String> {
    let emit = |msg: Arc<str>| {
        penv.monitor.emit(msg.clone());
        model_monitor.emit(msg);
    };
    emit("Building the model".into());
    let model = logjuicer_model::Model::<logjuicer_model::FeaturesMatrix>::train::<
        logjuicer_model::FeaturesMatrixBuilder,
    >(target_env, vec![content])
    .map_err(|e| {
        let msg = format!("Training the model failed: {:?}", e);
        emit(msg.clone().into());
        msg
    })?;
    target_env.clear_current();

    emit("Saving the model".into());
    let path = crate::models::save_model(&penv.storage_dir, content_id, &model)?;
    let size = FileSize::from(path.as_path());

    // Add the model to the db
    penv.handle
        .block_on(penv.db.add_model(content_id, size))
        .map_err(|e| format!("Adding the model to the db failed: {:?}", e))?;
    Ok(model)
}
