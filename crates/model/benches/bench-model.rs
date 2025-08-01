// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{criterion_group, criterion_main, Criterion};
use logjuicer_iterator::BytesLines;
use logjuicer_model::source::LinesIterator;
use std::hint::black_box;

use logjuicer_generate::gen_lines;

pub fn model_process(c: &mut Criterion) {
    let config = &logjuicer_model::config::TargetConfig::default();
    let lines = gen_lines().take(2048).collect::<Vec<String>>();
    let baselines = lines[0..42].join("\n");
    let target = lines[1024..2048].join("\n");

    let builder = logjuicer_index::FeaturesMatrixBuilder::default();
    let index = logjuicer_model::process::IndexTrainer::single(
        builder,
        false,
        config,
        std::io::Cursor::new(baselines),
    )
    .unwrap();

    c.bench_function("anomalies_from_reader", |b| {
        b.iter(|| {
            let data = std::io::Cursor::new(&target);
            let mut skip_lines = Some(logjuicer_model::unordered::KnownLines::new());
            let skip_lines = (
                &mut skip_lines,
                std::sync::Arc::new(std::sync::Mutex::new(None)),
            );
            let reader = LinesIterator::Bytes(BytesLines::new(black_box(data), false));
            let processor = logjuicer_model::process::ChunkProcessor::new(
                black_box(reader),
                black_box(&index),
                false,
                skip_lines,
                config,
                None,
            );
            let _anomalies = processor.collect::<Result<Vec<_>, _>>().unwrap();
        })
    });
}

criterion_group!(benches, model_process);
criterion_main!(benches);
