// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logreduce_generate::gen_lines;
use logreduce_tokenizer::process;

pub fn lexer_process(c: &mut Criterion) {
    let input = gen_lines().take(202).collect::<Vec<String>>().join("\n");
    c.bench_function("parser::process", |b| b.iter(|| process(black_box(&input))));
}

criterion_group!(benches, lexer_process);
criterion_main!(benches);
