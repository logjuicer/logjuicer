// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![allow(unused_variables)]
#![allow(dead_code)]
#![allow(unused_imports)]

use fxhash::hash32;
// use fasthash::murmur3::hash32;
use bincode::{deserialize, serialize};
use itertools::Itertools;
use sprs::*;
use std::collections::HashMap;

pub mod traits;

pub type F = f32;
type SparseVec = CsVecBase<Vec<usize>, Vec<F>, F>;
pub type FeaturesMatrix = CsMatBase<F, usize, Vec<usize>, Vec<usize>, Vec<F>>;
pub type FeaturesMatrixView<'a> = CsMatView<'a, F>;

/// A SparseVec with the norm pre computed
#[derive(Debug)]
pub struct Features {
    norm: F,
    vector: SparseVec,
}

fn into_feature(line: &str) -> Features {
    let vector = vectorize(line);
    Features {
        norm: vector.dot(&vector),
        vector,
    }
}

/// A simple index implementation, used for testing.
pub fn index_list(lines: &mut impl Iterator<Item = String>) -> Vec<Features> {
    lines.map(|line| into_feature(&line)).collect()
}

/// Compute the distance of a given line to a list of features, returns a number between 0.0 and 1.0
/// (0. means that the line is in the baseline)
pub fn search_list(baselines: &[Features], line: &str) -> F {
    let features = into_feature(line);
    1.0 - baselines.iter().fold(0.0, |acc, baseline| {
        similarity(&features, baseline).max(acc)
    })
}

pub fn save_mat(mat: &FeaturesMatrix) -> Vec<u8> {
    serialize(mat).unwrap()
}

pub fn load_mat(buf: &[u8]) -> FeaturesMatrix {
    deserialize(buf).unwrap()
}

/// Index implementation using a csr matrix storage.
/// Use the [`FeaturesMatrixBuilder`] for a streaming implementation.
pub fn index_mat(lines: &[String]) -> FeaturesMatrix {
    create_mat(&lines.iter().map(|s| vectorize(s)).collect::<Vec<_>>())
}

/// A simple search implementation, used for testing/benchmark.
/// Use the [`search_mat_chunk`] instead.
pub fn search_mat(baselines: &FeaturesMatrixView, lines: &[String]) -> Vec<F> {
    let target_vectors = lines.iter().map(|s| vectorize(s)).collect::<Vec<_>>();
    let mut targets = create_mat(&target_vectors);
    targets.transpose_mut();
    let mut result = vec![1.0; targets.cols()];
    cosine_distance(baselines, &targets, &mut result);
    result
}

/// Another impementation using baselines chunk
pub fn search_mat_chunk(baselines: &FeaturesMatrixView, lines: &[String]) -> Vec<F> {
    let target_vectors = lines.iter().map(|s| vectorize(s)).collect::<Vec<_>>();
    let mut targets = create_mat(&target_vectors);
    targets.transpose_mut();
    cosine_distance_chunk(baselines, &targets)
}

fn cosine_distance_chunk(
    baselines_chunks: &FeaturesMatrixView,
    targets: &FeaturesMatrix,
) -> Vec<F> {
    // The targets are transposed, the column is the log line number.
    let mut result = vec![1.0; targets.cols()];

    let max = baselines_chunks.rows();
    let mut start = 0;
    while start < max {
        let range = start..(start + 512).min(max);
        start += 512;

        let baselines = baselines_chunks.slice_outer(range);
        cosine_distance(&baselines, targets, &mut result)
    }
    result
}

pub struct FeaturesMatrixBuilder {
    current_row: usize,
    row: Vec<usize>,
    col: Vec<usize>,
    val: Vec<f32>,
}

impl traits::IndexReader for FeaturesMatrix {
    fn distance(&self, targets: &[String]) -> Vec<f32> {
        search_mat_chunk(&self.view(), targets)
    }
    fn rows(&self) -> usize {
        self.rows()
    }
}

impl traits::IndexBuilder for FeaturesMatrixBuilder {
    type Reader = FeaturesMatrix;

    fn add(&mut self, line: &str) {
        let row = self.current_row;
        self.current_row += 1;
        let vector = vectorize(line);
        let l2_norm = vector.l2_norm();
        for (col, val) in vector.iter() {
            self.row.push(row);
            self.col.push(col);
            self.val.push(*val / l2_norm);
        }
    }

    fn build(self) -> FeaturesMatrix {
        TriMat::from_triplets((self.row.len(), SIZE), self.row, self.col, self.val).to_csr()
    }
}

impl Default for FeaturesMatrixBuilder {
    fn default() -> Self {
        Self {
            current_row: 0,
            row: Vec::with_capacity(65535),
            col: Vec::with_capacity(65535),
            val: Vec::with_capacity(65535),
        }
    }
}

/// Create a normalized matrix
fn create_mat(vectors: &[SparseVec]) -> FeaturesMatrix {
    let mut mat = TriMat::new((vectors.len(), SIZE));
    for (row, vector) in vectors.iter().enumerate() {
        let l2_norm = vector.l2_norm();
        for (col, val) in vector.iter() {
            mat.add_triplet(row, col, *val / l2_norm);
        }
    }
    mat.to_csr()
}

/// Compute the cosine distance between two noramlized matrix.
/// Update the result argument when the distance is lower.
fn cosine_distance(baselines: &FeaturesMatrixView, targets: &FeaturesMatrix, result: &mut [F]) {
    // The targets are transposed, the column is the log line number.
    let distances_mat = baselines * targets;
    distances_mat
        .iter()
        .for_each(|(v, (_, col))| result[col] = (1.0 - v).min(result[col]))
}

const SIZE: usize = 260000;

// result = vector()
// for each word:
//    result[hash(word)] = 1
fn vectorize(line: &str) -> SparseVec {
    let (keys, values) = line
        .split(' ')
        .map(|word| {
            let hash = hash32(word);
            // alternate sign to improve inner product preservation in the hashed space
            let sign = if hash >= 2147483648 { 1.0 } else { -1.0 };
            ((hash as usize) % SIZE, sign)
        })
        .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
        // Here we sum the duplicate, but turns out,
        // it seems like sklearn hashing vectorizer doesn't do that. e.g.:
        // >>> from sklearn.feature_extraction.text import HashingVectorizer
        // >>> HashingVectorizer().transform(["abc abc"])[0,158726]
        // -1.0
        /*.dedup_with_count()
        .map(|(value, (key, sign))| (key, sign * value as F))*/
        .dedup_by(|a, b| a.0 == b.0)
        .unzip();
    CsVec::new(SIZE, keys, values)
}

/// Returns a number between 1.0 and 0.0, 0.0 being the closest value.
fn similarity(a: &Features, b: &Features) -> F {
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
        assert!((0.8..0.9).contains(&dist));
    }

    #[test]
    fn test_search() {
        let mut baselines = IntoIterator::into_iter([
            "the first line",
            "the second line",
            "the third line is a warning",
        ])
        .map(|s| s.to_string());
        let model = index_list(&mut baselines);
        assert!(dbg!(search_list(&model, "a new error")) > 0.6);
        assert_eq!(search_list(&model, "the second line"), 0.0);
    }

    #[test]
    fn test_search_mat() {
        let baselines = vec![
            "the first line".to_string(),
            "the second line".to_string(),
            "the third line is a warning".to_string(),
        ];
        let targets = vec!["a new error".to_string(), "the second line".to_string()];
        let model = index_mat(&baselines);
        let model = &model.view();
        let distances = search_mat(model, &targets);
        // The first target is definitely not in the baseline
        let expected = vec![0.7642977, 0.000000059604645];
        assert_eq!(distances, expected);

        let distances = search_mat_chunk(model, &targets);
        assert_eq!(distances, expected);
    }

    // A test playground that was used for the search_mat implementation
    #[test]
    fn test_matrix() {
        let baselines =
            IntoIterator::into_iter(["the", "the second line", "the third line is a warning"])
                .map(vectorize)
                .collect::<Vec<SparseVec>>();
        let baselines_mat = dbg!(create_mat(&baselines));

        let targets = IntoIterator::into_iter(["the second line", "a error"])
            .map(vectorize)
            .collect::<Vec<SparseVec>>();
        let mut targets_mat = dbg!(create_mat(&targets));
        targets_mat.transpose_mut();
        dbg!(&targets_mat);

        let mut distances_mat = &baselines_mat * &targets_mat;
        distances_mat.transpose_mut();
        let distances_mat_dense = dbg!(distances_mat.to_dense());

        let distances = distances_mat_dense
            .outer_iter()
            .map(|row| row.iter().fold(1.0, |acc: F, v| acc.min(1.0 - v)))
            .collect::<Vec<_>>();

        dbg!(distances);
        assert!(true)
    }
}
