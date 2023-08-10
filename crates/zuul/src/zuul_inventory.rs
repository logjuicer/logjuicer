// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use url::Url;

/// A Zuul inventory
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct InventoryRoot {
    pub all: InventoryAll,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct InventoryAll {
    pub vars: InventoryVars,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct InventoryVars {
    pub zuul: InventoryVarsZuul,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct InventoryVarsZuul {
    pub build: String,
    pub branch: String,
    pub job: String,
    pub pipeline: String,
    pub change_url: Url,
    pub project: InventoryProject,
    pub tenant: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct InventoryProject {
    pub name: String,
}

#[test]
fn test_decodes_manifest() {
    let data = r#"
all:
  hosts:
    fedora-37:
      ansible_connection: kubectl
  vars:
    zuul:
      branch: master
      build: 902789976f6349709955cb9f8786ba1d
      buildset: 0ed1c7ed2d1a419b8b13ad7fcc340376
      change: '28454'
      change_url: https://softwarefactory-project.io/r/28454
      job: ansible-lint
      pipeline: check
      project:
        canonical_hostname: softwarefactory-project.io
        canonical_name: softwarefactory-project.io/software-factory/sf-operator
        name: software-factory/sf-operator
      tenant: local
"#;
    let inventory: InventoryRoot = serde_yaml::from_str(data).unwrap();
    assert_eq!(
        inventory.all.vars.zuul.project.name,
        "software-factory/sf-operator"
    );
}
