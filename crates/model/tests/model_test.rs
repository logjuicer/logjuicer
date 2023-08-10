// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use logreduce_model::{Content, Source};
use std::path::PathBuf;
use std::str::FromStr;

#[test]
fn it_group_by_indexname() {
    let contents = include_str!("./sf-operator-hiearchy.txt")
        .split("\n")
        .filter(|s| !s.is_empty())
        .map(|s| {
            Content::File(Source::Local(
                0,
                PathBuf::from_str(s).unwrap().to_path_buf(),
            ))
        })
        .collect::<Vec<Content>>();
    let expected = include_str!("./sf-operator-hiearchy-group.txt");
    let mut got = String::with_capacity(4096);
    for (index_name, sources) in Content::group_sources(&contents)
        .unwrap()
        .drain()
        .sorted_by(|x, y| Ord::cmp(&x.0, &y.0))
    {
        got.push_str(&format!("{:}:", index_name));
        for source in sources {
            got.push_str(&format!(" {:}", source))
        }
        got.push('\n')
    }
    assert_eq!(got, expected);
}
