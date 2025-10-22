// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use logjuicer_index::{
    traits::{IndexBuilder as _, IndexReader},
    FeaturesMatrix, FeaturesMatrixBuilder,
};
use logjuicer_report::{AnomalyContext, IndexName, Report, Source};

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
pub fn filter_anomalies<IR: IndexReader>(
    index: &IR,
    source: &Source,
    anomalies: Vec<AnomalyContext>,
) -> Vec<AnomalyContext> {
    let mut lines = Vec::with_capacity(anomalies.len());
    let check_before = source.is_ansible();
    for anomaly in &anomalies {
        if check_before {
            // Compute the distance of the before context.
            for before in &anomaly.before {
                lines.push(logjuicer_tokenizer::process(before));
            }
        }
        lines.push(logjuicer_tokenizer::process(&anomaly.anomaly.line));
    }
    let distances = index.distance(&lines);
    let mut fresh = Vec::with_capacity(lines.len());
    let mut pos = 0;
    for anomaly in anomalies {
        let mut skip = true;
        if check_before {
            // Check if the before context is fresh.
            for _before in &anomaly.before {
                if skip && distances[pos] > crate::process::THRESHOLD {
                    skip = false;
                }
                pos += 1
            }
        }
        if !skip || distances[pos] > crate::process::THRESHOLD {
            fresh.push(anomaly)
        }
        pos += 1
    }
    fresh
}

#[test]
fn test_filter_anomalies() {
    let config = &crate::config::TargetConfig::default();
    let data = std::io::Cursor::new(
        r#"
2025-10-22 10:02:43.255665 | TASK [Susbscription manager check]
2025-10-22 10:02:43.304194 | primary | ERROR
2025-10-22 10:02:43.304485 | primary | {
nop
nop
2025-10-23 10:02:43.255665 | TASK [Second task]
2025-10-23 10:02:43.304194 | primary | ERROR
2025-10-23 10:02:43.304485 | primary | {
"#,
    );
    let reader =
        crate::source::LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(data, false));
    let skip_lines = std::sync::Arc::new(std::sync::Mutex::new(None));
    let processor = crate::errors::ErrorsProcessor::new(reader, skip_lines, config);
    let anomalies = processor
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        anomalies,
        vec![
            AnomalyContext {
                before: vec![
                    "2025-10-22 10:02:43.255665 | TASK [Susbscription manager check]".into()
                ],
                anomaly: logjuicer_report::Anomaly {
                    distance: 0.5,
                    pos: 3,
                    timestamp: Some(logjuicer_report::Epoch(1761127363304)),
                    line: "2025-10-22 10:02:43.304194 | primary | ERROR".into()
                },
                after: vec![
                    "2025-10-22 10:02:43.304485 | primary | {".into(),
                    "nop".into(),
                    "nop".into(),
                ]
            },
            AnomalyContext {
                before: vec!["2025-10-23 10:02:43.255665 | TASK [Second task]".into()],
                anomaly: logjuicer_report::Anomaly {
                    distance: 0.5,
                    pos: 8,
                    timestamp: Some(logjuicer_report::Epoch(1761213763304)),
                    line: "2025-10-23 10:02:43.304194 | primary | ERROR".into()
                },
                after: vec!["2025-10-23 10:02:43.304485 | primary | {".into()]
            }
        ]
    );

    let baseline = std::io::Cursor::new(
        r#"
2025-10-22 10:02:43.255665 | TASK [Susbscription manager check]
2025-10-22 10:02:43.304194 | primary | ERROR
2025-10-22 10:02:43.304485 | primary | {
"#,
    );
    let source = Source::from_pathbuf(std::path::PathBuf::from("logs/job-output.txt"));
    let mut index_trainer =
        crate::process::IndexTrainer::new(logjuicer_index::FeaturesMatrixBuilder::default());
    index_trainer
        .add_errors(
            config,
            &source,
            crate::source::LinesIterator::Bytes(logjuicer_iterator::BytesLines::new(
                baseline, false,
            )),
        )
        .expect("it indexs");

    let filtered = filter_anomalies(&index_trainer.build(), &source, anomalies.clone());
    assert_eq!(filtered, vec![anomalies[1].clone()]);
}

// Filter a report from a list of baselines
pub fn filter(baselines: &[Report], mut target: Report) -> Report {
    let indexes = index_baselines(baselines);
    let mut log_reports = Vec::with_capacity(target.log_reports.len());
    for mut lr in target.log_reports {
        if let Some(index) = indexes.get(&lr.index_name) {
            let fresh_anomalies = filter_anomalies(index, &lr.source, lr.anomalies);
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
