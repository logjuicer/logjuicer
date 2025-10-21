// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use logjuicer_index::{
    traits::{IndexBuilder as _, IndexReader as _},
    FeaturesMatrix, FeaturesMatrixBuilder,
};
use logjuicer_report::{AnomalyContext, IndexName, Report};

type Baselines = HashMap<IndexName, FeaturesMatrix>;

// Create a model from a list of report.
fn index_baselines(baselines: &[Report]) -> Baselines {
    let mut builders: HashMap<IndexName, FeaturesMatrixBuilder> = HashMap::new();
    for baseline in baselines {
        for lr in &baseline.log_reports {
            let builder = builders.entry(lr.index_name.clone()).or_default();
            for anomaly in &lr.anomalies {
                let tokens = logjuicer_tokenizer::process(&anomaly.anomaly.line);
                builder.add(&tokens)
            }
        }
    }
    let mut baselines = HashMap::new();
    for (index, builder) in builders.drain() {
        baselines.insert(index, builder.build());
    }
    baselines
}

// Filter anomalies not found in the model
fn filter_anomalies(index: &FeaturesMatrix, anomalies: Vec<AnomalyContext>) -> Vec<AnomalyContext> {
    let mut lines = Vec::with_capacity(anomalies.len());
    for anomaly in &anomalies {
        lines.push(logjuicer_tokenizer::process(&anomaly.anomaly.line));
    }
    let distances = index.distance(&lines);
    let mut fresh = Vec::with_capacity(lines.len());
    for (pos, anomaly) in anomalies.into_iter().enumerate() {
        if distances[pos] > crate::process::THRESHOLD {
            fresh.push(anomaly)
        }
    }
    fresh
}

// Filter a report from a list of baselines
pub fn filter(baselines: &[Report], mut target: Report) -> Report {
    let indexes = index_baselines(baselines);
    let mut log_reports = Vec::with_capacity(target.log_reports.len());
    for mut lr in target.log_reports {
        if let Some(index) = indexes.get(&lr.index_name) {
            let fresh_anomalies = filter_anomalies(index, lr.anomalies);
            if !fresh_anomalies.is_empty() {
                lr.anomalies = fresh_anomalies;
                log_reports.push(lr)
            }
        } else {
            log_reports.push(lr)
        }
    }
    target.log_reports = log_reports;
    target
}
