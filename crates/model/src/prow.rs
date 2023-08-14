// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::Read;
use url::Url;

use crate::{Baselines, Content, Source};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Build {
    pub url: Url,
    pub uuid: String,
    pub job_name: String,
    pub project: String,
    pub pr: u64,
}

impl std::fmt::Display for Build {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url.as_str())
    }
}

fn is_prow_uid(uid: &str) -> bool {
    !uid.contains(|c: char| !c.is_ascii_digit())
}

fn parse_prow_url(url: &Url) -> Option<Result<Build>> {
    url.path_segments().and_then(|mut iter| {
        // Check if the last segment is a uuid
        iter.next_back().and_then(|uid| match is_prow_uid(uid) {
            false => None,
            true => iter.next_back().and_then(|job_name| {
                iter.next_back().and_then(|pr| match pr.parse() {
                    Err(e) => Some(Err(anyhow::anyhow!("{}: invalid pr number {}", e, pr))),
                    Ok(pr) => iter.next_back().map(|project| {
                        Ok(Build {
                            url: url.clone(),
                            uuid: uid.to_string(),
                            job_name: job_name.to_string(),
                            project: project.to_string(),
                            pr,
                        })
                    }),
                })
            }),
        })
    })
}

#[test]
fn test_parse_prow_url() {
    let url = Url::parse("https://prow.ci.openshift.org/view/gs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792").unwrap();
    let res = parse_prow_url(&url).unwrap().unwrap();
    assert_eq!(
        res,
        Build {
            url: url,
            uuid: "1689624623181729792".to_string(),
            job_name: "pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test".to_string(),
            project: "openstack-k8s-operators_ci-framework".to_string(),
            pr: 437,
        }
    );
}

impl Content {
    pub fn from_prow_url(url: &Url) -> Option<Result<Content>> {
        match url.authority() {
            "prow.ci.openshift.org" => {
                parse_prow_url(url).map(|res| res.map(|build| Content::Prow(Box::new(build))))
            }
            _ => None,
        }
    }
}

fn get_prow_artifact_url(build: &Build) -> Result<Url> {
    let mut reader = crate::reader::from_url(&build.url, &build.url)?;
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;

    lazy_static::lazy_static! {
        static ref RE: Regex =
            Regex::new(r#"<a href="(http[^"]+)">Artifacts</a>"#).unwrap();
    }
    match RE.captures(&buffer) {
        None => Err(anyhow::anyhow!(
            "{}: could not find artifacts link in {}",
            build.url.as_str(),
            buffer
        )),
        Some(c) => Url::parse(c.get(1).unwrap().as_str()).context("Can't recreate artifact url"),
    }
}

#[test]
fn test_get_prow_artifact_url() -> Result<()> {
    let mut server = mockito::Server::new();
    let build = Build {
        url: Url::parse(&server.url())?,
        job_name: "test".to_string(),
        pr: 42,
        project: "proj".to_string(),
        uuid: "42".to_string(),
    };
    let base_mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_body(
            r#"<html>
               <div id="lens-container">
                 <div id="links-card" class="mdl-card mdl-shadow--2dp lens-card">
                   <a href="/job-history/gs/origin-ci-test/pr-logs/directory/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test">Job History</a>
                   <a href="/pr-history?org=openstack-k8s-operators&amp;repo=ci-framework&amp;pr=437">PR History</a>
                   <a href="https://artifacts.example.com/the-build/437/">Artifacts</a>
                 </div>
               </div>
               </html>"#,
        )
        .expect(1)
        .create();
    crate::reader::drop_url(&build.url, &build.url)?;

    let artifact_url = get_prow_artifact_url(&build).expect("Artifact url");
    assert_eq!(
        artifact_url.as_str(),
        "https://artifacts.example.com/the-build/437/"
    );
    base_mock.assert();

    Ok(())
}

impl Build {
    pub fn discover_prow_baselines(&self) -> Result<Baselines> {
        Ok(vec![])
    }

    pub fn sources_prow_iter(&self) -> Box<dyn Iterator<Item = Result<Source>>> {
        match get_prow_artifact_url(self) {
            Err(e) => Box::new(std::iter::once(Err(e))),
            Ok(url) => Source::httpdir_iter(&url),
        }
    }
}
