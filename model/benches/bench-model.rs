// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logreduce_generate::gen_lines;

pub fn model_process(c: &mut Criterion) {
    let lines = gen_lines().take(2048).collect::<Vec<String>>();
    let baselines = lines[0..42].join("\n");
    let target = lines[1024..2048].join("\n");

    let mut index = logreduce_model::hashing_index::new();
    logreduce_model::process::ChunkTrainer::single(
        &mut index,
        false,
        std::io::Cursor::new(baselines),
    )
    .unwrap();

    c.bench_function("anomalies_from_reader", |b| {
        b.iter(|| {
            let data = std::io::Cursor::new(&target);
            let mut skip_lines = std::collections::HashSet::new();
            let processor = logreduce_model::process::ChunkProcessor::new(
                black_box(data),
                &index,
                false,
                &mut skip_lines,
            );
            let _anomalies = processor.collect::<Result<Vec<_>, _>>().unwrap();
        })
    });
}

criterion_group!(benches, model_process);
criterion_main!(benches);
