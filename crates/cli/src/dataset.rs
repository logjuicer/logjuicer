// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic for the test command.
//! See the <https://github.com/logjuicer/logjuicer-tests> project

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::iter::zip;
use std::path::Path;

use logjuicer_model::env::Env;
use logjuicer_model::{content_from_pathbuf, AnomalyContext, IndexName, Model, Source};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct DatasetAnomaly {
    line: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Dataset {
    anomalies: Vec<DatasetAnomaly>,
}

fn load_inf(path: &Path) -> Result<Dataset> {
    let inf_path = path.join("inf.yaml");
    println!("Validating: {:?}", inf_path);
    let file = std::fs::File::open(inf_path)?;
    Ok(serde_yaml::from_reader(file)?)
}

pub fn test_datasets(env: &Env, paths: &[String]) -> Result<()> {
    for path_str in paths {
        let path = Path::new(&path_str);
        let inf = load_inf(path)?;
        process(env, path, inf)?
    }
    Ok(())
}

fn process(env: &Env, path: &Path, dataset: Dataset) -> Result<()> {
    let expected_count = dataset.anomalies.len();
    let paths = std::fs::read_dir(path)?
        .map(|d| d.unwrap().path())
        .collect::<Vec<std::path::PathBuf>>();
    match (
        paths
            .iter()
            .find(|p| p.extension() == Some(OsStr::new("good"))),
        paths
            .iter()
            .find(|p| p.extension() == Some(OsStr::new("fail"))),
    ) {
        (Some(good), Some(fail)) => {
            let model = Model::<logjuicer_model::FeaturesMatrix>::train::<
                logjuicer_model::FeaturesMatrixBuilder,
            >(env, [content_from_pathbuf(good.to_path_buf())].to_vec())?;
            let index = model.get_index(&IndexName::new()).unwrap();
            let anomalies = index
                .inspect(
                    env,
                    &Source::from_pathbuf(fail.to_path_buf()),
                    &mut logjuicer_model::unordered::KnownLines::new(),
                )
                .collect::<Result<Vec<AnomalyContext>>>()?;
            let anomalies_count = anomalies.len();
            for (expected, anomaly) in zip(dataset.anomalies, anomalies) {
                assert_anomaly_includes(expected.line, anomaly)?
            }
            if anomalies_count != expected_count {
                Err(anyhow::anyhow!(
                    "Expect miss-match: expected {}, got {}",
                    expected_count,
                    anomalies_count,
                ))
            } else {
                Ok(())
            }
        }
        _err => Err(anyhow::anyhow!(
            "Can't find .good and .fail files in {:?}",
            paths
        )),
    }
}

fn assert_anomaly_includes(line: String, anomaly: AnomalyContext) -> Result<()> {
    if anomaly.anomaly.line.contains(line.trim()) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Expected {}, got {:#?}", line, anomaly))
    }
}
