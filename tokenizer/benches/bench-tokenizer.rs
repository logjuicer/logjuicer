// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logreduce_tokenizer::tokenizer::{process};

pub fn lexer_process(c: &mut Criterion) {
    let input = include_str!("../../LICENSE");
    c.bench_function("parser::process", |b| {
        b.iter(|| process(black_box(input)))
    });
}

criterion_group!(benches, lexer_process);
criterion_main!(benches);
