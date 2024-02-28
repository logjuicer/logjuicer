// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This library provides data types for [zuul-ci](https://zuul-ci.org).
//!
//! # Installation
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! zuul-build = "0.1"
//! ```
//!
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

pub mod zuul_inventory;
pub mod zuul_manifest;

/// A Build result from Zuul v10 API.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct NewBuild {
    /// The build unique id.
    pub uuid: Box<str>,
    /// The job name.
    pub job_name: Box<str>,
    /// The job result.
    pub result: Box<str>,
    /// The start time.
    #[serde(with = "python_utc_without_trailing_z")]
    pub start_time: DateTime<Utc>,
    /// The end time.
    #[serde(with = "python_utc_without_trailing_z")]
    pub end_time: DateTime<Utc>,
    /// The job duration in second.
    #[serde(with = "rounded_float")]
    pub duration: u32,
    /// The job voting status.
    pub voting: bool,
    /// The log url.
    pub log_url: Option<Url>,
    /// The build artifacts.
    pub artifacts: Vec<Artifact>,
    /// The build pipeline.
    pub pipeline: Box<str>,
    #[serde(rename = "ref")]
    pub build_ref: BuildRef,
    /// The internal event id.
    pub event_id: Option<Box<str>>,
}

/// A Build reference from Zuul v10 API.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BuildRef {
    /// The ref url.
    pub ref_url: Option<Url>,
    /// The change's project name.
    pub project: Box<str>,
    /// The change's branch name.
    pub branch: Box<str>,
    /// The change (or PR) number.
    pub change: Option<u64>,
    /// The patchset number (or PR commit).
    pub patchset: Option<Box<str>>,
    /// The change ref.
    #[serde(rename = "ref")]
    pub change_ref: Box<str>,
}

impl NewBuild {
    fn convert_to_old_build(self) -> Build {
        Build {
            uuid: self.uuid,
            job_name: self.job_name,
            result: self.result,
            start_time: self.start_time,
            end_time: self.end_time,
            duration: self.duration,
            voting: self.voting,
            log_url: self.log_url,
            ref_url: self.build_ref.ref_url,
            artifacts: self.artifacts,
            project: self.build_ref.project,
            branch: self.build_ref.branch,
            pipeline: self.pipeline,
            change: self.build_ref.change,
            patchset: self.build_ref.patchset,
            change_ref: self.build_ref.change_ref,
            event_id: self.event_id,
        }
    }
}

/// A Build result.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Build {
    /// The build unique id.
    pub uuid: Box<str>,
    /// The job name.
    pub job_name: Box<str>,
    /// The job result.
    pub result: Box<str>,
    /// The start time.
    #[serde(with = "python_utc_without_trailing_z")]
    pub start_time: DateTime<Utc>,
    /// The end time.
    #[serde(with = "python_utc_without_trailing_z")]
    pub end_time: DateTime<Utc>,
    /// The job duration in second.
    #[serde(with = "rounded_float")]
    pub duration: u32,
    /// The job voting status.
    pub voting: bool,
    /// The log url.
    pub log_url: Option<Url>,
    /// The ref url.
    pub ref_url: Option<Url>,
    /// The build artifacts.
    pub artifacts: Vec<Artifact>,
    /// The change's project name.
    pub project: Box<str>,
    /// The change's branch name.
    pub branch: Box<str>,
    /// The build pipeline.
    pub pipeline: Box<str>,
    /// The change (or PR) number.
    pub change: Option<u64>,
    /// The patchset number (or PR commit).
    pub patchset: Option<Box<str>>,
    /// The change ref.
    #[serde(rename = "ref")]
    pub change_ref: Box<str>,
    /// The internal event id.
    pub event_id: Option<Box<str>>,
}

/// A Build artifact.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Artifact {
    /// The artifact name.
    pub name: String,
    /// The artifact url.
    pub url: Url,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum BuildResult {
    Legacy(Build),
    New(NewBuild),
}

impl BuildResult {
    fn convert_to_build(self) -> Build {
        match self {
            BuildResult::Legacy(legacy) => legacy,
            BuildResult::New(nb) => nb.convert_to_old_build(),
        }
    }
}

pub fn decode_build<R: std::io::Read>(reader: R) -> serde_json::Result<Build> {
    serde_json::from_reader(reader).map(|br: BuildResult| br.convert_to_build())
}

pub fn decode_builds<R: std::io::Read>(reader: R) -> serde_json::Result<Vec<Build>> {
    serde_json::from_reader(reader).map(|xs: Vec<serde_json::Value>| {
        xs.into_iter()
            // Sometime the API returns builds without uuid.
            // So we filter the builds that don't deserialize.
            .filter_map(|v| {
                serde_json::from_value(v)
                    .map(|br: BuildResult| br.convert_to_build())
                    .ok()
            })
            .collect()
    })
}

// Copy pasta from https://serde.rs/custom-date-format.html
mod python_utc_without_trailing_z {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let naive =
            chrono::NaiveDateTime::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)?;
        Ok(DateTime::from_naive_utc_and_offset(naive, Utc))
    }
}

// For some reason, durations are sometime provided as f32, e.g. `42.0`
mod rounded_float {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &u32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(*duration)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u32, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = f32::deserialize(deserializer)?;
        Ok(v as u32)
    }
}

#[test]
fn test_decodes_build() {
    let data = r#"
            {
              "uuid": "5bae5607ae964331bb5878aec0777637",
              "job_name": "hlint",
              "result": "SUCCESS",
              "start_time": "2021-10-13T12:57:20",
              "end_time": "2021-10-13T12:58:42",
              "duration": 82.0,
              "voting": true,
              "log_url": "https://softwarefactory-project.io/logs/94/22894/1/gate/hlint/5bae560/",
              "artifacts": [
                {
                  "name": "Zuul Manifest",
                  "url": "https://softwarefactory-project.io/logs/94/22894/1/gate/hlint/5bae560/zuul-manifest.json",
                  "metadata": {
                    "type": "zuul_manifest"
                  }
                },
                {
                  "name": "HLint report",
                  "url": "https://softwarefactory-project.io/logs/94/22894/1/gate/hlint/5bae560/hlint.html"
                }
              ],
              "project": "software-factory/matrix-client-haskell",
              "branch": "master",
              "pipeline": "gate",
              "change": 22894,
              "patchset": "1",
              "ref": "refs/changes/94/22894/1",
              "ref_url": "https://softwarefactory-project.io/r/22894",
              "event_id": "40d9b63d749c48eabb3d7918cfab0d31"
            }"#;
    let build: Build = decode_build(data.as_bytes()).unwrap();
    assert_eq!(build.uuid.as_ref(), "5bae5607ae964331bb5878aec0777637");
}

#[test]
fn test_decodes_new_build() {
    let data = r#"
{
  "_id": 23404081,
  "uuid": "e09e82136d714711a5d01051bc633e65",
  "job_name": "nodepool-functional-container-openstack-release",
  "result": "FAILURE",
  "held": false,
  "start_time": "2024-02-13T23:17:28",
  "end_time": "2024-02-13T23:39:24",
  "duration": 1316.0,
  "voting": true,
  "log_url": "https://localhost/log_url",
  "nodeset": "ubuntu-jammy",
  "error_detail": null,
  "final": true,
  "artifacts": [],
  "provides": [],
  "ref": {
    "project": "zuul/nodepool",
    "branch": "master",
    "change": 908952,
    "patchset": "1",
    "ref": "refs/changes/52/908952/1",
    "oldrev": null,
    "newrev": null,
    "ref_url": "https://review.opendev.org/908952"
  },
  "pipeline": "check",
  "event_id": "9991d429cacc495c9cde0f5dfe874f8e",
  "event_timestamp": "2024-02-13T22:13:12",
  "buildset": {
    "uuid": "338114acf62841f48d37862efc29cb87",
    "refs": [
      {
        "project": "zuul/nodepool",
        "branch": "master",
        "change": 908952,
        "patchset": "1",
        "ref": "refs/changes/52/908952/1",
        "oldrev": null,
        "newrev": null,
        "ref_url": "https://review.opendev.org/908952"
      }
    ]
  }
}"#;
    let build: Build = decode_build(data.as_bytes()).unwrap();
    assert_eq!(build.uuid.as_ref(), "e09e82136d714711a5d01051bc633e65");
}
