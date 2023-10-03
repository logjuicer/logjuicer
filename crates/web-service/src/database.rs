// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use logreduce_report::{Content, Report};

pub struct Database {
    pub running: HashSet<Arc<str>>,
    pub failed: Vec<(Arc<str>, Box<str>)>,
    pub reports: Vec<Content>,
}

fn load_reports() -> Vec<Content> {
    vec![]
}

impl Database {
    fn new() -> Self {
        Database {
            running: HashSet::new(),
            failed: Vec::new(),
            reports: load_reports(),
        }
    }

    fn completed(&mut self, base_url: Arc<str>, result: Result<Report, Box<str>>) {
        let _ = self.running.remove(&base_url);
        match result {
            Ok(report) => {
                if let Err(err) = report.save(Path::new("todo")) {
                    self.failed
                        .push((base_url, format!("Failed to save report: {:?}", err).into()))
                } else {
                    self.reports.push(report.target);
                }
            }
            Err(err) => self.failed.push((base_url, err)),
        }
    }
}

#[derive(Clone)]
pub struct Workers {
    pool: threadpool::ThreadPool,
    pub database: Arc<Mutex<Database>>,
}

impl Workers {
    pub fn new() -> Self {
        Workers {
            pool: threadpool::ThreadPool::new(2),
            database: Arc::new(Mutex::new(Database::new())),
        }
    }

    pub fn submit(&self, base_url: &str) {
        let url: Arc<str> = base_url.into();
        let mut db = self.database.lock().unwrap();
        // Check if the report is being processed
        if db.running.insert(url.clone()) {
            let db = self.database.clone();
            self.pool.execute(move || {
                println!("Starting processing of {}!", url);
                let result = process_report(&url);
                db.lock().unwrap().completed(url, result);
                println!("Processing completed!");
            })
        }
    }
}

fn process_report(_target: &Arc<str>) -> Result<Report, Box<str>> {
    std::thread::sleep(std::time::Duration::from_secs(5));
    Err("oops".into())
}

use base64::{engine::general_purpose, Engine as _};
fn url_path(url: &str) -> String {
    let mut buf = "data/".to_string();
    general_purpose::STANDARD_NO_PAD.encode_string(url, &mut buf);
    buf
}

pub fn report_path(url: &str) -> String {
    let mut fp = url_path(url);
    fp.push_str(".bin");
    fp
}

pub fn info_path(url: &str) -> String {
    let mut fp = url_path(url);
    fp.push_str(".inf");
    fp
}
