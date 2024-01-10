// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logjuicer_generate::gen_lines;
use logjuicer_index::*;

pub fn process(c: &mut Criterion) {
    let lines = gen_lines().take(4096).collect::<Vec<String>>();
    let baselines = &lines[0..2048];
    let targets = &lines;

    let model = index_mat(baselines);
    let model = model.view();
    /*
    // This benchmark measure searching 64 targets in 512 baseines.
    // The goal is to compare per-line search versus chunk search.
    // The results show that chunk search is multiple order of magnitude faster:
    // search_individual       time:   [21.832 ms 21.883 ms 21.938 ms]
    // search_chunk            time:   [389.57 us 390.79 us 392.19 us]
    c.bench_function("search_individual", |b| {
        b.iter(|| {
            targets.iter().for_each(|target| {
                search_mat1(black_box(&model), black_box(target));
            })
        })
    });
    */
    c.bench_function("search_chunk", |b| {
        b.iter(|| {
            search_mat_chunk(black_box(&model), black_box(targets));
        })
    });
}

criterion_group!(benches, process);
criterion_main!(benches);
