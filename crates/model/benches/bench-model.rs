// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logreduce_generate::gen_lines;

pub fn model_process(c: &mut Criterion) {
    let lines = gen_lines().take(2048).collect::<Vec<String>>();
    let baselines = lines[0..42].join("\n");
    let target = lines[1024..2048].join("\n");

    let index =
        logreduce_model::process::IndexTrainer::single(false, std::io::Cursor::new(baselines))
            .unwrap();

    c.bench_function("anomalies_from_reader", |b| {
        b.iter(|| {
            let data = std::io::Cursor::new(&target);
            let mut skip_lines = logreduce_model::unordered::KnownLines::new();
            let processor = logreduce_model::process::ChunkProcessor::new(
                black_box(data),
                black_box(&index),
                false,
                false,
                &mut skip_lines,
            );
            let _anomalies = processor.collect::<Result<Vec<_>, _>>().unwrap();
        })
    });
}

criterion_group!(benches, model_process);
criterion_main!(benches);
