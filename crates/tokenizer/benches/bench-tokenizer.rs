// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use logjuicer_generate::gen_lines;
use logjuicer_tokenizer::process;

pub fn lexer_process(c: &mut Criterion) {
    let input = gen_lines().take(202).collect::<Vec<String>>().join("\n");
    c.bench_function("parser::process", |b| b.iter(|| process(black_box(&input))));
}

criterion_group!(benches, lexer_process);
criterion_main!(benches);
