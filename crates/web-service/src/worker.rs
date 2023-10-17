// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;

use logreduce_model::env::Env;
use logreduce_report::report_row::{ReportID, ReportStatus};
use logreduce_report::Report;

use crate::database::Db;

#[derive(Clone)]
pub struct Workers {
    /// The execution pool to run logreduce model.
    pool: threadpool::ThreadPool,
    /// The report process monitor to broadcast the status to websocket clients.
    running: Arc<RwLock<BTreeMap<ReportID, ProcessMonitor>>>,
    /// The logreduce environment.
    env: Arc<Env>,
    /// The local database of reports.
    pub db: Db,
}

const MAX_LOGREDUCE_PROCESS: usize = 2;

impl Workers {
    pub async fn new() -> Self {
        // TODO: requeue pending build
        Workers {
            db: Db::new().await.unwrap(),
            pool: threadpool::ThreadPool::new(MAX_LOGREDUCE_PROCESS),
            env: Arc::new(Env::new()),
            running: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn subscribe(&self, report_id: ReportID) -> Option<ProcessMonitor> {
        let running = self.running.read().unwrap();
        running.get(&report_id).cloned()
    }

    // TODO: deny this clippy warning
    #[allow(clippy::map_entry)]
    pub fn submit(&self, report_id: ReportID, target: &str) {
        let mut running_init_write = self.running.write().unwrap();
        // Check if the report is being processed
        if !running_init_write.contains_key(&report_id) {
            println!("Submiting");
            let monitor = ProcessMonitor::new();
            running_init_write.insert(report_id, monitor.clone());
            std::mem::drop(running_init_write);

            // Prepare worker variables
            let env = self.env.clone();
            let target = target.to_string();
            let running = self.running.clone();
            let db = self.db.clone();
            let handle = tokio::runtime::Handle::current();

            // Submit the execution to the thread pool
            self.pool.execute(move || {
                let (status, count) = match process_report(&env, &target, &monitor) {
                    Ok(report) => {
                        let count = report.anomaly_count();
                        let fp = format!("data/{}.bin", report_id);
                        let status = if let Err(err) = report.save(std::path::Path::new(&fp)) {
                            monitor.emit(format!("Error: saving failed: {}", err).into());
                            ReportStatus::Error(format!("Save error: {}", err))
                        } else {
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
                let _ = running.write().unwrap().remove(&report_id);
                // Record the result into the db
                handle.spawn(
                    async move { db.update_report(report_id, count, &status).await.unwrap() },
                );
            })
        } else {
            println!("Already submitted");
        }
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
        println!("Emitting {}", msg);
        self.events.blocking_write().push(msg.clone());
        let _ = self.chan.send(msg);
    }
}

fn process_report(env: &Env, target: &str, monitor: &ProcessMonitor) -> Result<Report, String> {
    monitor.emit(format!("Running `logreduce url {}`", target).into());
    let input = logreduce_model::Input::Url(target.into());
    let content =
        logreduce_model::content_from_input(env, input).map_err(|e| format!("{:?}", e))?;

    monitor.emit(format!("Content resolved: {}", content).into());
    let baselines = logreduce_model::content_discover_baselines(&content, env)
        .map_err(|e| format!("discovery failed: {:?}", e))?;

    monitor.emit(format!("Baseline found: {}", baselines.iter().format(", ")).into());
    let model = logreduce_model::Model::train(env, baselines, logreduce_model::hashing_index::new)
        .map_err(|e| format!("training failed: {:?}", e))?;

    monitor.emit("Starting analysis".into());
    let report = model
        .report(env, content)
        .map_err(|e| format!("report failed: {:?}", e))?;
    Ok(report)
}
