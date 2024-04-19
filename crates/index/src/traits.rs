// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

pub trait IndexBuilder {
    type Reader;

    fn add(&mut self, line: &str);
    fn build(self) -> Self::Reader;
}

pub trait IndexReader {
    /// Return the number of feature vectors.
    fn rows(&self) -> usize;

    /// Compute the distances of the given lines.
    fn distance(&self, lines: &[String]) -> Vec<f32>;

    /// Combine two indexes.
    fn mappend(&self, other: &Self) -> Self;

    /// Combine multiple indexes.
    fn mconcat(&self, others: &[&Self]) -> Self;
}
