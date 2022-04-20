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

/// A Build result.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Build {
    /// The build unique id.
    pub uuid: String,
    /// The job name.
    pub job_name: String,
    /// The job result.
    pub result: String,
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
    pub project: String,
    /// The change's branch name.
    pub branch: String,
    /// The build pipeline.
    pub pipeline: String,
    /// The change (or PR) number.
    pub change: Option<u64>,
    /// The patchset number (or PR commit).
    pub patchset: Option<String>,
    /// The change ref.
    #[serde(rename = "ref")]
    pub change_ref: String,
    /// The internal event id.
    pub event_id: String,
}

/// A Build artifact.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Artifact {
    /// The artifact name.
    pub name: String,
    /// The artifact url.
    pub url: Url,
}

pub fn decode_build<R: std::io::Read>(reader: R) -> serde_json::Result<Build> {
    serde_json::from_reader(reader)
}

pub fn decode_builds<R: std::io::Read>(reader: R) -> serde_json::Result<Vec<Build>> {
    serde_json::from_reader(reader).map(|xs: Vec<serde_json::Value>| {
        xs.into_iter()
            // Sometime the API returns builds without uuid.
            // So we filter the builds that don't deserialize.
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect()
    })
}

// Copy pasta from https://serde.rs/custom-date-format.html
mod python_utc_without_trailing_z {
    use chrono::{DateTime, TimeZone, Utc};
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
        Utc.datetime_from_str(&s, FORMAT)
            .map_err(serde::de::Error::custom)
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
    let build: Build = serde_json::from_str(data).unwrap();
    assert_eq!(build.uuid, "5bae5607ae964331bb5878aec0777637");
}
