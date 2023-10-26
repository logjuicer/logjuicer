// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

pub trait IndexBuilder {
    type Reader;

    fn add(&mut self, line: &str);
    fn build(self) -> Self::Reader;
}

pub trait IndexReader {
    fn rows(&self) -> usize;
    fn distance(&self, lines: &[String]) -> Vec<f32>;
}
