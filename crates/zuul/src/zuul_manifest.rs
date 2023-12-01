// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use url::Url;

/// A Zuul manifest
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Manifest {
    pub tree: Vec<Tree>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Tree {
    pub name: Box<str>,
    pub mimetype: Box<str>,
    #[serde(default)]
    pub children: Vec<Tree>,
}

impl Manifest {
    pub fn to_urls(&self, mut url: Url) -> Vec<Url> {
        let mut res = vec![];
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        for tree in self.tree.iter() {
            go_tree(tree, &mut res, &url);
        }
        res
    }
}

fn go_tree(tree: &Tree, res: &mut Vec<Url>, url: &Url) {
    if tree.mimetype.as_ref() == "application/directory" {
        let dir_url = url.join(&format!("{}/", tree.name)).expect("good url");
        for child in tree.children.iter() {
            go_tree(child, res, &dir_url)
        }
    } else {
        res.push(url.join(&tree.name).expect("good url"))
    }
}

#[test]
fn test_decodes_manifest() {
    let data = r#"
{
  "tree": [
    {
      "name": "zuul-info",
      "mimetype": "application/directory",
      "encoding": null,
      "children": [
        {
          "name": "host-info.controller.yaml",
          "mimetype": "text/plain",
          "encoding": null,
          "last_modified": 1701427487,
          "size": 14195
        }
      ]
    },
    {
      "name": "job-output.txt",
      "mimetype": "text/plain",
      "encoding": null,
      "last_modified": 1701427579,
      "size": 33375
    }
  ],
  "index_links": false
}
"#;
    let manifest: Manifest = serde_json::from_str(data).unwrap();
    assert_eq!(
        manifest
            .to_urls(Url::parse("http://localhost/build").unwrap())
            .iter()
            .map(|u| u.as_str())
            .collect::<Vec<&str>>(),
        vec![
            "http://localhost/build/zuul-info/host-info.controller.yaml",
            "http://localhost/build/job-output.txt"
        ]
    );
}
