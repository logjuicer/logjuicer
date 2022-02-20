// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_variables)]
#![allow(dead_code)]

use fxhash::hash;
use itertools::Itertools;
use sprs::*;
use std::collections::HashMap;

type SparseVec = CsVecBase<Vec<usize>, Vec<f32>, f32>;

/// A SparseVec with the norm pre computed
#[derive(Debug)]
pub struct Features {
    norm: f32,
    vector: SparseVec,
}

fn into_feature(line: &str) -> Features {
    let vector = vectorize(line);
    Features {
        norm: vector.dot(&vector),
        vector,
    }
}

/// Build a list of features
pub fn index(lines: &mut impl Iterator<Item = String>) -> Vec<Features> {
    lines.map(|line| into_feature(&line)).collect()
}

/// Compute the distance of a given line to a list of features, returns a number between 0.0 and 1.0
/// (0. means the line is in the baseline)
pub fn search(baselines: &[Features], line: &str) -> f32 {
    let features = into_feature(line);
    1.0 - baselines.iter().fold(0.0, |acc, baseline| {
        similarity(&features, baseline).max(acc)
    })
}

const SIZE: usize = 100000;

// result = vector()
// for each word:
//    result[hash(word)] = 1
fn vectorize(line: &str) -> SparseVec {
    let mut hashes = HashMap::new();
    for word in line.split(' ') {
        let key = hash(word) % SIZE;
        let count = match hashes.get(&key) {
            Some(prev) => *prev + 1.0,
            _ => 1.0,
        };
        hashes.insert(key, count as f32);
    }
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for key in hashes.keys().sorted() {
        keys.push(*key);
        values.push(*hashes.get(key).unwrap());
    }
    CsVec::new(SIZE, keys, values)
}

/// Returns a number between 1.0 and 0.0, 0.0 being the closest value.
fn similarity(a: &Features, b: &Features) -> f32 {
    let norms = a.norm * b.norm;
    (a.vector.dot(&b.vector)) / norms.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity() {
        let l1 = dbg!(into_feature("the first test is the 42"));
        let l2 = into_feature("the second test is the 42");
        assert_eq!(similarity(&l1, &l1), 1.0);
        let dist = dbg!(similarity(&l1, &l2));
        assert!(dist > 0.8 && dist < 0.9);
    }

    #[test]
    fn test_search() {
        let mut baselines = IntoIterator::into_iter([
            "the first line",
            "the second line",
            "the third line is a warning",
        ])
        .map(|s| s.to_string());
        let model = index(&mut baselines);
        assert!(dbg!(search(&model, "a new error")) > 0.6);
        assert_eq!(search(&model, "the second line"), 0.0);
    }
}
