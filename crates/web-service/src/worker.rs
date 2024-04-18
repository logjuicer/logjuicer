// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;

use logjuicer_model::env::EnvConfig;
use logjuicer_report::report_row::{ReportID, ReportStatus};
use logjuicer_report::Report;

use crate::database::Db;

type ReportsLock = Arc<RwLock<BTreeMap<ReportID, ProcessMonitor>>>;

#[derive(Clone)]
pub struct Workers {
    /// The execution pool to run logjuicer model.
    pool: threadpool::ThreadPool,
    /// The report process monitor to broadcast the status to websocket clients.
    reports: ReportsLock,
    /// The logjuicer environment.
    env: Arc<EnvConfig>,
    /// The local database of reports.
    pub db: Db,
}

const MAX_LOGJUICER_PROCESS: usize = 2;

impl Workers {
    pub async fn new(env: EnvConfig) -> Self {
        // TODO: requeue pending build
        Workers {
            db: Db::new().await.unwrap(),
            pool: threadpool::ThreadPool::new(MAX_LOGJUICER_PROCESS),
            env: Arc::new(env),
            reports: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn subscribe(&self, report_id: ReportID) -> Option<ProcessMonitor> {
        let running = self.reports.read().unwrap();
        running.get(&report_id).cloned()
    }

    // TODO: deny this clippy warning
    #[allow(clippy::map_entry)]
    pub fn submit(&self, report_id: ReportID, target: &str, baseline: Option<&str>) {
        if let Some(monitor) = report_lock(report_id, &self.reports) {
            tracing::info!("Submiting new url {}", target);

            // Prepare worker variables
            let env = self.env.clone();
            let target = target.to_string();
            let baseline = baseline.map(|s| s.to_string());
            let reports = self.reports.clone();
            let db = self.db.clone();
            let handle = tokio::runtime::Handle::current();

            // Submit the execution to the thread pool
            self.pool.execute(move || {
                let baseline = baseline.as_deref();
                let (status, count) = match process_report_safe(&env, &target, baseline, &monitor) {
                    Ok(report) => {
                        let count = report.anomaly_count();
                        let fp = format!("data/{}.gz", report_id);
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
    env: &EnvConfig,
    target: &str,
    baseline: Option<&str>,
    monitor: &ProcessMonitor,
) -> Result<Report, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        process_report(env, target, baseline, monitor)
    })) {
        Ok(res) => res,
        Err(err) => Err(format!(
            "crashed {}",
            err.downcast::<&str>().unwrap_or(Box::new("unknown"))
        )),
    }
}

fn process_report(
    env: &EnvConfig,
    target: &str,
    baseline: Option<&str>,
    monitor: &ProcessMonitor,
) -> Result<Report, String> {
    match baseline {
        None => monitor.emit(format!("Running `logjuicer url {}`", target).into()),
        Some(baseline) => {
            monitor.emit(format!("Running `logjuicer diff {} {}`", baseline, target).into())
        }
    }

    use logjuicer_report::Content;
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
    check_content(&content)?;

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
    baselines.iter().try_for_each(check_content)?;

    let target_env = env.get_target_env(&content);

    let model = logjuicer_model::Model::<logjuicer_model::FeaturesMatrix>::train::<
        logjuicer_model::FeaturesMatrixBuilder,
    >(&target_env, baselines)
    .map_err(|e| format!("training failed: {:?}", e))?;

    monitor.emit("Starting analysis".into());
    let report = model
        .report(&target_env, content)
        .map_err(|e| format!("report failed: {:?}", e))?;
    Ok(report)
}
