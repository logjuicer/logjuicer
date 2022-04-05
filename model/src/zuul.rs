// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_imports)]
use anyhow::{Context, Result};
use chrono::{Date, DateTime, Utc};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{Baselines, Content, Source};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Build {
    api: Option<Url>,
    pub uuid: Option<String>,
    pub job_name: String,
    pub project: String,
    pub branch: String,
    pub result: String,
    pub duration: Option<f32>,
    pub pipeline: String,
    pub voting: bool,
    pub log_url: Option<Url>,
    #[serde(with = "python_utc_without_trailing_z")]
    pub end_time: DateTime<Utc>,
    pub ref_url: Url,
    pub change: Option<usize>,
}

impl std::fmt::Display for Build {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}build/{}",
            self.api.as_ref().map(|a| a.as_str()).unwrap_or("api"),
            self.uuid.as_ref().unwrap_or(&"unknown".to_string())
        )
    }
}

fn elapsed_days(now: &Date<Utc>, since: Date<Utc>) -> i32 {
    let days = now.signed_duration_since(since).num_days();
    if days < 0 {
        0
    } else {
        days as _
    }
}

impl Build {
    pub fn get_api(&self) -> Result<&Url> {
        self.api
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Build needs api url for baseline discovery"))
    }

    pub fn get_success_samples(&self) -> Result<Vec<Build>> {
        let api = self.get_api()?;
        let base = api.join("builds").context("Can't create builds url")?;
        let url = Url::parse_with_params(
            base.as_str(),
            [
                ("job_name", self.job_name.as_str()),
                // ("complete", "true"),
                ("limit", "500"),
                ("result", "SUCCESS"),
            ],
        )
        .context("Can't create query url")?;
        tracing::info!(url = url.as_str(), "Discovering baselines for {}", self);
        get_builds(api, &url)
    }

    fn baseline_score(&self, target: &Build, now: &Date<Utc>) -> Option<i32> {
        let mut score = 0;
        // Sanity check
        self.uuid.as_ref()?;
        self.log_url.as_ref()?;
        // Rules
        if self.project == target.project {
            if self.change == target.change {
                // We don't want to compare with the same change
                score -= 500;
            } else {
                score += 50;
            }
        }
        if self.branch == target.branch {
            score += 50;
        }
        if target.pipeline.contains("gate") || target.pipeline.contains("periodic") {
            score += 50;
        }
        if target.voting {
            score += 10;
        }
        // Older builds are less valuable
        score -= elapsed_days(now, target.end_time.date());
        Some(score)
    }

    pub fn discover_baselines(&self) -> Result<Baselines> {
        let samples = self.get_success_samples()?;
        let api = self.get_api()?;
        let max_builds = 1;
        let now = Utc::now().date();
        Ok(samples
            .into_iter()
            // Compute a score value
            .map(|build| (self.baseline_score(&build, &now), build))
            // Remove unwanted build
            .filter(|(score, build)| score.is_some() && self.uuid != build.uuid)
            // Order by descending score
            .sorted_by(|(score1, _), (score2, _)| score2.cmp(score1))
            // .map(|b| dbg!(b))
            // Keep the best
            .take(max_builds)
            // Create the content data type
            .map(|(_score, build)| new_content(api.clone(), build))
            .collect())
    }

    pub fn sources_iter(&self) -> Box<dyn Iterator<Item = Result<Source>>> {
        match &self.log_url {
            None => Box::new(std::iter::once(Err(anyhow::anyhow!(
                "Build is missing log_url"
            )))),
            Some(url) => Source::httpdir_iter(url),
        }
    }
}

fn new_content(api: Url, build: Build) -> Content {
    Content::Zuul(Box::new(Build {
        api: Some(api),
        ..build
    }))
}

fn get_build(api: &Url, uid: &str) -> Result<Build> {
    let url = api.join("build/")?.join(uid)?;
    let reader = crate::reader::from_url(api, &url)?;
    match serde_json::from_reader(reader).context("Can't decode zuul api") {
        Ok(x) => Ok(x),
        Err(e) => crate::reader::drop_url(api, &url).map_or_else(Err, |_| Err(e)),
    }
}

fn get_builds(api: &Url, url: &Url) -> Result<Vec<Build>> {
    let reader = crate::reader::from_url(api, url)?;
    match serde_json::from_reader(reader).context("Can't decode zuul builds api") {
        Ok(x) => Ok(x),
        Err(e) => crate::reader::drop_url(api, url).map_or_else(Err, |_| Err(e)),
    }
}

fn is_uid(s: &str) -> bool {
    s.len() == 32
        && !s.contains(|c| {
            !('a'..='z').contains(&c) && !('A'..='Z').contains(&c) && !('0'..='9').contains(&c)
        })
}

fn api_from_webui(url: &Url, tenant: &str) -> Result<Url> {
    url.as_str()
        .split_once("/t/")
        .ok_or_else(|| anyhow::anyhow!("Invalid zuul url"))
        .and_then(|(base, _)| {
            Url::parse(&format!("{}/api/tenant/{}/", base, tenant))
                .context("Can't recreate zuul api url")
        })
}

fn api_from_whitelabel_webui(url: &Url) -> Result<Url> {
    url.as_str()
        .rsplit_once("/build/")
        .ok_or_else(|| anyhow::anyhow!("Invalid zuul url"))
        .and_then(|(base, _)| {
            Url::parse(&format!("{}/api/", base)).context("Can't recreate zuul api url")
        })
}

fn get_zuul_api_url(url: &'_ Url) -> Option<Result<(Url, &'_ str)>> {
    url.path_segments().and_then(|mut iter| {
        // Check if the last segment is a uuid
        iter.next_back().and_then(|uid| match is_uid(uid) {
            false => None,
            // Check if the next segment is "/build/"
            true => iter.next_back().and_then(|build| match build == "build" {
                false => None,
                true => iter
                    .next_back()
                    .and_then(|tenant| match iter.next_back() {
                        // This is a multi tenant url
                        Some("t") => Some(api_from_webui(url, tenant).map(|api| (api, uid))),
                        _ => None,
                    })
                    .or_else(|| {
                        // Otherwise assume this is a whitelabel url
                        Some(api_from_whitelabel_webui(url).map(|api| (api, uid)))
                    }),
            }),
        })
    })
}

impl Content {
    pub fn from_zuul_url(url: &Url) -> Option<Result<Content>> {
        get_zuul_api_url(url).map(|res| {
            res.and_then(|(api, uid)| get_build(&api, uid).map(|build| new_content(api, build)))
        })
    }
}

#[test]
fn test_zuul_url() {
    let assert_url = |full, base, uid: &str| {
        let url = Url::parse(full).unwrap();
        let content = get_zuul_api_url(&url).unwrap().unwrap();
        let expected = (Url::parse(base).unwrap(), uid);
        assert_eq!(content, expected);
    };

    assert_url(
        "https://zuul.opendev.org/t/zuul/build/a498f74ab32b49ffa9c9e7463fbf8885",
        "https://zuul.opendev.org/api/tenant/zuul/",
        "a498f74ab32b49ffa9c9e7463fbf8885",
    );

    assert_url(
        "https://review.rdoproject.org/zuul/build/2498d287ec4b442a95184b7a4bec9b2d",
        "https://review.rdoproject.org/zuul/api/",
        "2498d287ec4b442a95184b7a4bec9b2d",
    );
}

#[test]
fn test_zuul_api() -> Result<()> {
    use mockito::mock;
    let url = Url::parse(&mockito::server_url())?;
    let build_url = url.join("/zuul/build/2498d287ec4b442a95184b7a4bec9b2d")?;
    let api_path = "/zuul/api/build/2498d287ec4b442a95184b7a4bec9b2d";
    let base_mock = mock("GET", api_path)
        .with_body(
            r#"{
              "uuid": "a498f74ab32b49ffa9c9e7463fbf8885",
              "job_name": "zuul-tox-py38-multi-scheduler",
              "result": "FAILURE",
              "voting": false,
              "log_url": "https://localhost/42",
              "final": true,
              "project": "zuul/zuul",
              "branch": "master",
              "pipeline": "check",
              "duration": 42,
              "change": 1,
              "ref_url": "https://review.opendev.org/835662",
              "end_time": "2014-07-08T09:10:11"
            }"#,
        )
        .expect(1)
        .create();
    let catch_all = mock("GET", mockito::Matcher::Any)
        .with_body("oops")
        .expect(0)
        .create();

    crate::reader::drop_url(&url.join("/zuul/api/")?, &url.join(api_path)?)?;
    let content = Content::from_zuul_url(&build_url).unwrap()?;
    let expected = Content::Zuul(Box::new(Build {
        api: Some(url.join("/zuul/api/")?),
        uuid: Some("a498f74ab32b49ffa9c9e7463fbf8885".to_string()),
        job_name: "zuul-tox-py38-multi-scheduler".to_string(),
        result: "FAILURE".to_string(),
        voting: false,
        duration: Some(42.0),
        log_url: Some(Url::parse("https://localhost/42")?),
        project: "zuul/zuul".to_string(),
        branch: "master".to_string(),
        pipeline: "check".to_string(),
        ref_url: Url::parse("https://review.opendev.org/835662")?,
        change: Some(1),
        end_time: "2014-07-08T09:10:11Z".parse().unwrap(),
    }));
    assert_eq!(content, expected);

    catch_all.assert();
    base_mock.assert();

    Ok(())
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
