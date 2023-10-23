// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logreduce_generate::gen_lines;
use logreduce_report::*;

fn mk_report() -> Report {
    // Create a fake report with 1k anomaly
    let lines = gen_lines().take(1024).collect::<Vec<String>>();
    let mut report = Report::sample();
    let anomalies = &mut report.log_reports.get_mut(0).unwrap().anomalies;
    lines.iter().enumerate().for_each(|(pos, line)| {
        anomalies.push(AnomalyContext {
            before: vec!["before".into()],
            anomaly: Anomaly {
                distance: 0.42,
                pos,
                line: line.as_str().into(),
            },
            after: vec!["after".into()],
        })
    });
    report
}

fn mk_capnp(report: &Report) -> Vec<u8> {
    let mut capnp_buffer = Vec::with_capacity(65535);
    report.save_writer(&mut capnp_buffer).unwrap();
    capnp_buffer
}

fn mk_bincode(report: &Report) -> Vec<u8> {
    let mut bincode_buffer = Vec::with_capacity(65535);
    bincode::serialize_into(&mut bincode_buffer, report).unwrap();
    bincode_buffer
}

fn mk_json(report: &Report) -> Vec<u8> {
    let mut json_buffer = Vec::with_capacity(65535);
    serde_json::to_writer(&mut json_buffer, report).unwrap();
    json_buffer
}

fn bench_read(c: &mut Criterion) {
    let report = &mk_report();
    let encoded_capnp: Vec<u8> = mk_capnp(report);
    let encoded_bincode: &[u8] = &mk_bincode(report);
    let encoded_json: &[u8] = &mk_json(report);

    let mut group = c.benchmark_group("Read");
    group.bench_function("capnp", |b| {
        b.iter(|| {
            // Create a message reader
            let mut slice: &[u8] = black_box(&encoded_capnp);
            let message_reader = capnp::serialize::read_message_from_flat_slice(
                &mut slice,
                capnp::message::ReaderOptions::new(),
            )
            .unwrap();
            let reader = message_reader
                .get_root::<logreduce_report::schema_capnp::report::Reader<'_>>()
                .unwrap();

            // Traverse the list of log reports
            let count = reader
                .get_log_reports()
                .unwrap()
                .iter()
                .fold(0, |acc, lr| acc + lr.get_anomalies().unwrap().len());
            assert_eq!(count, 1025)
        })
    });
    group.bench_function("bincode", |b| {
        b.iter(|| {
            let slice: &[u8] = black_box(&encoded_bincode);
            let report: Report = bincode::deserialize_from(slice).unwrap();
            let count = report
                .log_reports
                .iter()
                .fold(0, |acc, lr| acc + lr.anomalies.len());
            assert_eq!(count, 1025)
        })
    });
    group.bench_function("json", |b| {
        b.iter(|| {
            let slice: &[u8] = black_box(&encoded_json);
            let report: Report = serde_json::from_reader(slice).unwrap();
            let count = report
                .log_reports
                .iter()
                .fold(0, |acc, lr| acc + lr.anomalies.len());
            assert_eq!(count, 1025)
        })
    });
    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let report = &mk_report();

    // Encode the report with capnp for decode bench
    let encoded_capnp: &[u8] = &mk_capnp(report);
    // std::fs::write("report-capnp.bin", encoded_capnp).unwrap();

    // Encode the report with bincode for decode bench
    let encoded_bincode: &[u8] = &mk_bincode(report);
    // std::fs::write("report-bincode.bin", encoded_bincode).unwrap();

    // Encode the report with json
    let encoded_json: &[u8] = &mk_json(report);
    // std::fs::write("report.json", encoded_json).unwrap();

    let mut group = c.benchmark_group("Decoder");
    group.bench_function("capnp", |b| {
        b.iter(|| {
            let _report = Report::load_bufreader(&encoded_capnp[..]).unwrap();
        })
    });
    group.bench_function("bincode", |b| {
        b.iter(|| {
            let _report: Report = bincode::deserialize_from(encoded_bincode).unwrap();
        })
    });
    group.bench_function("json", |b| {
        b.iter(|| {
            let _report: Report = serde_json::from_reader(encoded_json).unwrap();
        })
    });
    group.finish();
}

fn bench_encode(c: &mut Criterion) {
    let report = &mk_report();
    let mut buffer = std::io::Cursor::new(Vec::with_capacity(65535));

    let mut group = c.benchmark_group("Encoder");
    group.bench_function("capnp", |b| {
        b.iter(|| {
            black_box(report).save_writer(&mut buffer).unwrap();
            buffer.set_position(0);
        })
    });
    group.bench_function("bincode", |b| {
        b.iter(|| {
            bincode::serialize_into(&mut buffer, black_box(report)).unwrap();
            buffer.set_position(0);
        })
    });
    group.bench_function("json", |b| {
        b.iter(|| {
            serde_json::to_writer(&mut buffer, black_box(report)).unwrap();
            buffer.set_position(0);
        })
    });
    group.finish();
}

criterion_group!(benches, bench_decode, bench_encode, bench_read);
criterion_main!(benches);
