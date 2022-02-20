// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logreduce_generate::gen_lines;
use logreduce_index::*;

pub fn process(c: &mut Criterion) {
    let mut lines = gen_lines();
    let target = lines.next().unwrap();
    let model = index(&mut lines.take(17000));
    c.bench_function("search", |b| {
        b.iter(|| search(black_box(&model), black_box(&target)))
    });
}

criterion_group!(benches, process);
criterion_main!(benches);
