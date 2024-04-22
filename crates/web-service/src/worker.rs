// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use logjuicer_model::env::TargetEnv;
use logjuicer_model::ModelF;
use logjuicer_report::model_row::ContentID;
use logjuicer_report::Content;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;

use logjuicer_model::env::EnvConfig;
use logjuicer_report::report_row::{ReportID, ReportStatus};
use logjuicer_report::Report;

use crate::database::Db;

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
}

const MAX_LOGJUICER_PROCESS: usize = 2;

impl Workers {
    pub async fn new(allow_any_sources: bool, storage_dir: Arc<str>, env: EnvConfig) -> Self {
        // TODO: requeue pending build
        Workers {
            allow_any_sources,
            db: Db::new(&storage_dir).await.unwrap(),
            pool: threadpool::ThreadPool::new(MAX_LOGJUICER_PROCESS),
            env: Arc::new(env),
            reports: Arc::new(RwLock::new(BTreeMap::new())),
            models: Arc::new(RwLock::new(BTreeMap::new())),
            storage_dir,
        }
    }

    pub fn subscribe(&self, report_id: ReportID) -> Option<ProcessFollower> {
        let running = self.reports.read().unwrap();
        running.get(&report_id).map(|pm| ProcessFollower {
            events: pm.events.clone(),
            chan: pm.chan.subscribe(),
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

    // TODO: deny this clippy warning
    #[allow(clippy::map_entry)]
    pub fn submit(&self, report_id: ReportID, target: &str, baseline: Option<&str>) {
        if let Some(monitor) = report_lock(report_id, &self.reports) {
            tracing::info!("Submiting new url {}", target);

            // Prepare worker variables
            let env = self.env.clone();
            let models_lock = self.models.clone();
            let target = target.to_string();
            let baseline = baseline.map(|s| s.to_string());
            let reports = self.reports.clone();
            let db = self.db.clone();
            let handle = tokio::runtime::Handle::current();
            let storage_dir = self.storage_dir.clone();
            let allow_any_sources = self.allow_any_sources;

            // Submit the execution to the thread pool
            self.pool.execute(move || {
                let baseline = baseline.as_deref();
                let penv = ProcessEnv {
                    storage_dir,
                    monitor,
                    models_lock,
                    db: db.clone(),
                    handle: handle.clone(),
                    allow_any_sources,
                };
                let monitor = &penv.monitor;
                let (status, count) = match process_report_safe(&penv, &env, &target, baseline) {
                    Ok(report) => {
                        let count = report.anomaly_count();
                        let fp = format!("{}/{}.gz", penv.storage_dir, report_id);
                        let status = if let Err(err) = report.save(std::path::Path::new(&fp)) {
                            tracing::error!("{}: failed to save report: {}", fp, err);
                            monitor.emit(format!("Error: saving failed: {}", err).into());
                            ReportStatus::Error(format!("Save error: {}", err))
                        } else {
                            tracing::info!("{}: saved report", fp);
                            monitor.emit("Done".into());
                            ReportStatus::Completed
                        };
                        (status, count)
                    }
                    Err(e) => {
                        monitor.emit(format!("Error: {}", e).into());
                        (ReportStatus::Error(e), 0)
                    }
                };
                // Remove the monitor
                let _ = reports.write().unwrap().remove(&report_id);
                // Record the result into the db
                handle.spawn(
                    async move { db.update_report(report_id, count, &status).await.unwrap() },
                );
            })
        } else {
            tracing::info!("Url already submitted {}", target);
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
}

#[derive(Clone)]
pub struct ProcessMonitor {
    pub events: Arc<tokio::sync::RwLock<Vec<Arc<str>>>>,
    pub chan: tokio::sync::broadcast::Sender<Arc<str>>,
}

impl ProcessMonitor {
    fn new() -> Self {
        let (chan, _) = tokio::sync::broadcast::channel(16);
        ProcessMonitor {
            events: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            chan,
        }
    }

    fn emit(&self, msg: Arc<str>) {
        // println!("Emitting {}", msg);
        self.events.blocking_write().push(msg.clone());
        let _ = self.chan.send(msg);
    }
}

fn process_report_safe(
    penv: &ProcessEnv,
    env: &EnvConfig,
    target: &str,
    baseline: Option<&str>,
) -> Result<Report, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        process_report(penv, env, target, baseline)
    })) {
        Ok(res) => res,
        Err(err) => Err(format!(
            "crashed {}",
            err.downcast::<&str>().unwrap_or(Box::new("unknown"))
        )),
    }
}

fn process_report(
    penv: &ProcessEnv,
    env: &EnvConfig,
    target: &str,
    baseline: Option<&str>,
) -> Result<Report, String> {
    let monitor = &penv.monitor;
    match baseline {
        None => monitor.emit(format!("Running `logjuicer url {}`", target).into()),
        Some(baseline) => {
            monitor.emit(format!("Running `logjuicer diff {} {}`", baseline, target).into())
        }
    }

    fn check_content(content: &Content) -> Result<(), String> {
        match content {
            Content::Zuul(_) | logjuicer_report::Content::Prow(_) => Ok(()),
            _ => Err("Only zuul or prow build are supported".to_string()),
        }
    }

    let input = logjuicer_model::Input::Url(target.into());
    let content =
        logjuicer_model::content_from_input(&env.gl, input).map_err(|e| format!("{:?}", e))?;

    monitor.emit(format!("Content resolved: {}", content).into());
    if !penv.allow_any_sources {
        check_content(&content)?;
    }

    let baselines = match baseline {
        Some(baseline) => {
            let input = logjuicer_model::Input::Url(baseline.into());
            vec![logjuicer_model::content_from_input(&env.gl, input)
                .map_err(|e| format!("baseline: {:?}", e))?]
        }
        None => logjuicer_model::content_discover_baselines(&content, &env.gl)
            .map_err(|e| format!("discovery failed: {:?}", e))?,
    };

    monitor.emit(format!("Baseline found: {}", baselines.iter().format(", ")).into());
    if !penv.allow_any_sources {
        baselines.iter().try_for_each(check_content)?;
    }

    let target_env = env.get_target_env(&content);
    let model: ModelF = process_models(penv, &target_env, baselines)?;

    monitor.emit("Starting analysis".into());
    let report = model
        .report(&target_env, content)
        .map_err(|e| format!("report failed: {:?}", e))?;
    Ok(report)
}

fn process_models(
    penv: &ProcessEnv,
    target_env: &TargetEnv,
    baselines: Vec<Content>,
) -> Result<ModelF, String> {
    let mut models = baselines
        .into_iter()
        .map(|content| process_model(penv, target_env, content))
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

fn model_lock(penv: &ProcessEnv, content_id: &ContentID) -> Result<ModelStatus, String> {
    let mut models_lock = penv.models_lock.write().unwrap();
    match models_lock.get(content_id) {
        // Someone is already building it
        Some(monitor) => Ok(ModelStatus::Pending(ProcessFollower {
            events: monitor.events.clone(),
            chan: monitor.chan.subscribe(),
        })),
        // Nobody is building it
        None => match penv
            .handle
            .block_on(penv.db.lookup_model(content_id))
            .map_err(|e| format!("db model lookup: {}", e))?
        {
            // The model is not in the database
            None => {
                let monitor = ProcessMonitor::new();
                models_lock.insert(content_id.clone(), monitor.clone());
                Ok(ModelStatus::ToBuild(monitor))
            }
            // The model was already built
            Some(()) => Ok(ModelStatus::Existing),
        },
    }
}

fn process_model(
    penv: &ProcessEnv,
    target_env: &TargetEnv,
    content: Content,
) -> Result<ModelF, String> {
    let content_id = (&content).into();
    match model_lock(penv, &content_id)? {
        ModelStatus::Existing => {
            penv.monitor.emit("Loading existing model".into());
            crate::models::load_model(&penv.storage_dir, &content_id)
        }
        ModelStatus::Pending(mut model_follower) => {
            penv.handle.block_on(async {
                while let Ok(msg) = model_follower.chan.recv().await {
                    penv.monitor.emit(msg);
                }
            });
            crate::models::load_model(&penv.storage_dir, &content_id)
        }
        ModelStatus::ToBuild(model_monitor) => {
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

            emit("Saving the model".into());
            crate::models::save_model(&penv.storage_dir, &content_id, &model)?;

            // Add the model to the db
            penv.handle
                .block_on(penv.db.add_model(&content_id))
                .map_err(|e| format!("Adding the model to the db failed: {:?}", e))?;

            // Remove the monitor
            let _ = penv.models_lock.write().unwrap().remove(&content_id);

            Ok(model)
        }
    }
}
