// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
};

use itertools::Itertools;
use logjuicer_report::{
    LogReport, Report, SimilarityAnomalyContext, SimilarityLogReport, SimilarityReport,
    SimilaritySource, SourceID, TargetID,
};

// This function creates a 'SimilarityReport' from a list of 'Report'.
pub fn create_similarity_report(reports: &[&Report]) -> SimilarityReport {
    // Collect the report targets, their index are used as reference in the SimilaritySource.
    let targets = reports.iter().map(|report| report.target.clone()).collect();

    // Collect the baselines used for creating the reports, for tracability purpose.
    let baselines = reports
        .iter()
        .flat_map(|report| report.baselines.clone())
        .unique()
        .collect();

    // Group every LogReport per IndexName
    let mut log_reports = HashMap::new();
    reports.iter().enumerate().for_each(|(target_idx, report)| {
        report.log_reports.iter().for_each(|lr| {
            log_reports
                .entry(&lr.index_name)
                .or_insert_with(Vec::new)
                .push((TargetID(target_idx), lr))
        })
    });

    // Create the list of SimilarityLogReport
    let similarity_reports = log_reports
        .into_values()
        .map(|lr| create_similarity_log_report(&lr))
        // TODO: sort by maximum anomaly sources, not just global
        .sorted_by_key(|slr| -(slr.sources.len() as i32))
        .collect();

    SimilarityReport {
        targets,
        baselines,
        similarity_reports,
    }
}

// This function is the core implementation of 'create_similarity_report'. It creates a 'SimilarityLogReport'
// out of a list of LogReport.
fn create_similarity_log_report(log_reports: &[(TargetID, &LogReport)]) -> SimilarityLogReport {
    // Collect the sources
    let sources: Vec<SimilaritySource> = log_reports
        .iter()
        .map(|(target_idx, lr)| SimilaritySource {
            target: *target_idx,
            source: lr.source.clone(),
        })
        .collect();

    // Group the anomalies per target and compute their features vector sets
    let mut target_anomalies = HashMap::new();
    log_reports.iter().for_each(|(target_idx, log_report)| {
        // Pre-compute the tokenized version of the anomalies
        let tokens = log_report
            .anomalies
            .iter()
            .map(|ac| logjuicer_tokenizer::process(&ac.anomaly.line))
            .collect::<Vec<String>>();

        // HashSet the tokens for fast lookup
        let set: HashSet<String> = HashSet::from_iter(tokens.iter().cloned());

        // Store the source id location
        let source_id = sources
            .iter()
            .position(|s| s.target == *target_idx && s.source == log_report.source)
            .unwrap_or(0);

        // Group per target id
        target_anomalies
            .entry(target_idx)
            .or_insert_with(Vec::new)
            .push((log_report, tokens, SourceID(source_id), set))
    });

    // Collect the anomaly context
    let mut anomalies = vec![];
    let mut done = HashSet::new();
    let targets: Vec<&&TargetID> = target_anomalies.keys().sorted().collect();
    // For each target...
    for target in 0..targets.len() {
        let target_id = targets[target];

        // For each report in the current target...
        for (lr, tokens, source_id, _) in &target_anomalies[target_id] {
            // For each anomaly in the current report...
            for (anomaly, token) in lr.anomalies.iter().zip(tokens) {
                // Check if this is a new anomaly.
                if done.insert(token) {
                    let mut sources = HashSet::new();
                    sources.insert(*source_id);

                    // For each other target...
                    for other_target_id in targets.iter().skip(target + 1) {
                        // For each report in the other target
                        for (_, _, other_source_id, other_set) in
                            &target_anomalies[*other_target_id]
                        {
                            if other_set.contains(token) {
                                sources.insert(*other_source_id);
                            }
                        }
                    }
                    anomalies.push(SimilarityAnomalyContext {
                        sources: sources.into_iter().sorted().collect(),
                        anomaly: anomaly.clone(),
                    })
                }
            }
        }
    }

    SimilarityLogReport { sources, anomalies }
}

#[test]
fn test_report_similarity() {
    use logjuicer_report::*;
    use std::time::Duration;

    let r1 = Report::sample();
    let mut r2 = Report::sample();
    r2.target = Content::File(Source::Local(4, "/proc/test".into()));
    r2.log_reports[0].anomalies.push(AnomalyContext {
        before: vec![],
        after: vec![],
        anomaly: Anomaly {
            distance: 0.7,
            pos: 2,
            timestamp: None,
            line: "other anomaly".into(),
        },
    });
    r2.log_reports.push(LogReport {
        test_time: Duration::from_secs(42),
        line_count: 1,
        byte_count: 13,
        anomalies: vec![AnomalyContext {
            before: vec![],
            anomaly: Anomaly {
                distance: 0.5,
                pos: 1,
                timestamp: None,
                line: "extra".into(),
            },
            after: vec![],
        }],
        index_name: IndexName("test2".into()),
        source: Source::Local(4, "/proc/cmd".into()),
    });

    let sr = create_similarity_report(&[&r1, &r2]);
    assert_eq!(sr.targets, vec![r1.target, r2.target]);
    assert_eq!(sr.baselines, r1.baselines);
    let expected = vec![
        SimilarityLogReport {
            sources: vec![
                SimilaritySource {
                    target: TargetID(0),
                    source: Source::Local(4, "/proc/status".into()),
                },
                SimilaritySource {
                    target: TargetID(1),
                    source: Source::Local(4, "/proc/status".into()),
                },
            ],
            anomalies: vec![
                SimilarityAnomalyContext {
                    sources: vec![SourceID(0), SourceID(1)],
                    anomaly: AnomalyContext {
                        before: vec!["before".into(), "...".into()],
                        anomaly: Anomaly {
                            distance: 0.5,
                            pos: 1,
                            timestamp: None,
                            line: "anomaly".into(),
                        },
                        after: vec![],
                    },
                },
                SimilarityAnomalyContext {
                    sources: vec![SourceID(1)],
                    anomaly: AnomalyContext {
                        before: vec![],
                        anomaly: Anomaly {
                            distance: 0.7,
                            pos: 2,
                            timestamp: None,
                            line: "other anomaly".into(),
                        },
                        after: vec![],
                    },
                },
            ],
        },
        SimilarityLogReport {
            sources: vec![SimilaritySource {
                target: TargetID(1),
                source: Source::Local(4, "/proc/cmd".into()),
            }],
            anomalies: vec![SimilarityAnomalyContext {
                sources: vec![SourceID(0)],
                anomaly: AnomalyContext {
                    before: vec![],
                    anomaly: Anomaly {
                        distance: 0.5,
                        pos: 1,
                        timestamp: None,
                        line: "extra".into(),
                    },
                    after: vec![],
                },
            }],
        },
    ];
    assert_eq!(expected, sr.similarity_reports);
}
