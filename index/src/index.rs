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

pub type F = f32;
type SparseVec = CsVecBase<Vec<usize>, Vec<F>, F>;
pub type FeaturesMatrix = CsMatBase<F, usize, Vec<usize>, Vec<usize>, Vec<F>>;

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

/// Build a list of features
pub fn index(lines: &mut impl Iterator<Item = String>) -> Vec<Features> {
    lines.map(|line| into_feature(&line)).collect()
}

/// Compute the distance of a given line to a list of features, returns a number between 0.0 and 1.0
/// (0. means the line is in the baseline)
pub fn search(baselines: &[Features], line: &str) -> F {
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

/// Another implementation for index using a matrix storage
pub fn index_mat(lines: &[String]) -> FeaturesMatrix {
    create_mat(&lines.iter().map(|s| vectorize(s)).collect::<Vec<_>>())
}

/// Another implementation for search using a matrix product
pub fn search_mat(baselines: &FeaturesMatrix, lines: &[String]) -> Vec<F> {
    let target_vectors = lines.iter().map(|s| vectorize(s)).collect::<Vec<_>>();
    let mut targets = create_mat(&target_vectors);
    targets.transpose_mut();
    cosine_distance(baselines, &targets)
}

/// Another impementation using baselines chunk
pub fn search_mat_chunk(baselines: &[FeaturesMatrix], lines: &[String]) -> Vec<F> {
    let target_vectors = lines.iter().map(|s| vectorize(s)).collect::<Vec<_>>();
    let mut targets = create_mat(&target_vectors);
    targets.transpose_mut();
    cosine_distance_chunk(baselines, &targets)
}

fn cosine_distance_chunk(baselines: &[FeaturesMatrix], targets: &FeaturesMatrix) -> Vec<F> {
    // The targets are transposed, the column is the log line number.
    let mut result = vec![1.0; targets.cols()];

    baselines.iter().for_each(|baseline| {
        let distances_mat = baseline * targets;

        distances_mat
            .iter()
            .for_each(|(v, (_, col))| result[col] = (1.0 - v).min(result[col]));
    });
    result
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

/// Compute the cosine distance between two noramlized matrix
fn cosine_distance(baselines: &FeaturesMatrix, targets: &FeaturesMatrix) -> Vec<F> {
    // The targets are transposed, the column is the log line number.
    let mut result = vec![1.0; targets.cols()];
    let distances_mat = baselines * targets;
    distances_mat
        .iter()
        .for_each(|(v, (_, col))| result[col] = (1.0 - v).min(result[col]));
    result
}

const SIZE: usize = 260000;

// result = vector()
// for each word:
//    result[hash(word)] = 1
// TODO: vectorize directly into a FeaturesMatrix
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
        assert!(dist >= 0.8 && dist < 0.9);
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

    #[test]
    fn test_search_mat() {
        let baselines = vec![
            "the first line".to_string(),
            "the second line".to_string(),
            "the third line is a warning".to_string(),
        ];
        let targets = vec!["a new error".to_string(), "the second line".to_string()];
        let model = index_mat(&baselines);
        let distances = search_mat(&model, &targets);
        // The first target is definitely not in the baseline
        let expected = vec![0.7642977, 0.000000059604645];
        assert_eq!(distances, expected);

        let distances = search_mat_chunk(&[model], &targets);
        assert_eq!(distances, expected);
    }

    // A test playground that was used for the search_mat implementation
    #[test]
    fn test_matrix() {
        let baselines =
            IntoIterator::into_iter(["the", "the second line", "the third line is a warning"])
                .map(|s| vectorize(s))
                .collect::<Vec<SparseVec>>();
        let baselines_mat = dbg!(create_mat(&baselines));

        let targets = IntoIterator::into_iter(["the second line", "a error"])
            .map(|s| vectorize(s))
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
