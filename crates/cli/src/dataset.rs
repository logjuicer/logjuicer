// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic for the test command.
//! See the <https://github.com/logjuicer/logjuicer-tests> project

use anyhow::Result;
use logjuicer_report::Content;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::iter::zip;
use std::path::Path;

use logjuicer_model::env::{Env, EnvConfig, OutputMode};
use logjuicer_model::{content_from_pathbuf, AnomalyContext, IndexName, Model, Source};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct DatasetAnomaly {
    line: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Dataset {
    anomalies: Vec<DatasetAnomaly>,
    #[serde(default = "default_false")]
    skip: bool,
}

fn default_false() -> bool {
    false
}

fn load_inf(path: &Path) -> Result<Dataset> {
    let inf_path = path.join("inf.yaml");
    let file = std::fs::File::open(inf_path)?;
    Ok(serde_yaml::from_reader(file)?)
}

pub fn test_datasets(base_env: EnvConfig, paths: &[String]) -> Result<()> {
    let env = EnvConfig {
        gl: Env {
            output: OutputMode::Quiet,
            ..base_env.gl
        },
        ..base_env
    };
    let mut fail_count = 0;
    let mut success_count = 0;
    for path_str in paths {
        let path = Path::new(&path_str);
        println!("[+] Validating: {:?}", path);
        match load_inf(path) {
            Ok(inf) if inf.skip => println!("-> Skipped"),
            Ok(inf) => match process(&env, path, inf) {
                Ok(()) => {
                    println!("-> OK");
                    success_count += 1
                }
                Err(e) => {
                    println!("{}", e);
                    fail_count += 1
                }
            },
            Err(e) => {
                println!("-> Failed to read inf.yaml: {e}");
                fail_count += 1
            }
        }
    }
    if fail_count > 0 {
        println!("{fail_count}/{} tests failed", fail_count + success_count);
        std::process::exit(1)
    }
    println!("{success_count} tests succeeded");
    Ok(())
}

fn process(env: &EnvConfig, path: &Path, dataset: Dataset) -> Result<()> {
    let env = &env.get_target_env(&Content::sample("default"));
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
            let source = Source::from_pathbuf(fail.to_path_buf());
            let reader = logjuicer_model::source::open_raw_source(env.gl, &source)?;
            let anomalies = index
                .get_processor(
                    env,
                    &source,
                    reader,
                    &mut Some(logjuicer_model::unordered::KnownLines::new()),
                    None,
                )?
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
