// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use goldenfile::Mint;
use logjuicer_tokenizer::index_name::IndexName;
use std::io::Write;

#[test]
fn it_makes_indexname() {
    let mut mint = Mint::new("tests/");
    let mut expected = mint.new_goldenfile("index-list.txt").unwrap();
    include_str!("./files-list.txt")
        .split("\n")
        .filter(|s| !s.is_empty())
        .map(IndexName::from_path)
        .for_each(|indexname| {
            write!(expected, "{}\n", indexname.as_str()).unwrap();
        })
}
