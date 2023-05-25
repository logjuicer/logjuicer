// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides an unordered bag of lines

use itertools::Itertools;
use std::collections::HashSet;

#[derive(Debug, Eq, Hash, PartialEq)]
struct UnorderedLine(Vec<String>);

impl UnorderedLine {
    fn from_str(line: &str) -> UnorderedLine {
        UnorderedLine(
            line.split(' ')
                .sorted()
                .map(|word| word.to_string())
                .collect(),
        )
    }
}

#[derive(Debug)]
pub struct KnownLines(HashSet<UnorderedLine>);

impl KnownLines {
    pub fn new() -> KnownLines {
        KnownLines(HashSet::new())
    }

    pub fn insert(&mut self, line: &str) -> bool {
        let uline = UnorderedLine::from_str(line);
        self.0.insert(uline)
    }
}

impl Default for KnownLines {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_known_lines() {
    let mut skip_lines = KnownLines::new();
    assert_eq!(true, skip_lines.insert("first line"));
    assert_eq!(false, skip_lines.insert("first line"));
    assert_eq!(false, skip_lines.insert("line first"));
}
