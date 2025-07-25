// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use regex::Regex;
use std::io::Read;
use url::Url;

use crate::env::Env;
use crate::{Content, SourceLoc};
use logjuicer_report::ProwBuild;

fn is_prow_uid(uid: &str) -> bool {
    !uid.contains(|c: char| !c.is_ascii_digit())
}

fn parse_prow_url(url: &Url) -> Option<Result<ProwBuild>> {
    match url.path_segments()?.collect::<Vec<_>>()[..] {
        ["view", storage_type, storage_path, "pr-logs", "pull", project, pr, job, uid] => {
            match (is_prow_uid(uid), pr.parse()) {
                (true, Ok(pr)) => Some(Ok(ProwBuild {
                    url: url.clone(),
                    uid: uid.into(),
                    job_name: job.into(),
                    project: project.into(),
                    pr,
                    storage_type: storage_type.into(),
                    storage_path: storage_path.into(),
                })),
                (_, Err(e)) => Some(Err(anyhow::anyhow!("{}: invalid pr number {}", pr, e))),
                _ => Some(Err(anyhow::anyhow!(
                    "{}: couldn't decode build info",
                    url.as_str()
                ))),
            }
        }
        _ => None,
    }
}

#[test]
fn test_parse_prow_url() {
    let url = Url::parse("https://prow.ci.openshift.org/view/gs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792").unwrap();
    let res = parse_prow_url(&url).unwrap().unwrap();
    assert_eq!(
        res,
        ProwBuild {
            url: url,
            uid: "1689624623181729792".into(),
            job_name: "pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test".into(),
            project: "openstack-k8s-operators_ci-framework".into(),
            pr: 437,
            storage_type: "gs".into(),
            storage_path: "origin-ci-test".into(),
        }
    );
}

pub fn content_from_prow_url(url: &Url) -> Option<Result<Content>> {
    match url.authority() {
        "prow.ci.openshift.org" => {
            parse_prow_url(url).map(|res| res.map(|build| Content::Prow(Box::new(build))))
        }
        _ => None,
    }
}

fn get_prow_artifact_url(env: &Env, url: &Url) -> Result<Url> {
    let mut reader = crate::reader::get_url(env, url)?;
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;

    lazy_static::lazy_static! {
        static ref RE: Regex =
            Regex::new(r#"<a href="(http[^"]+)">Artifacts</a>"#).unwrap();
    }
    match RE.captures(&buffer) {
        None => Err(anyhow::anyhow!(
            "{}: could not find artifacts link in {}",
            url.as_str(),
            buffer
        )),
        Some(c) => Url::parse(c.get(1).unwrap().as_str()).context("Can't recreate artifact url"),
    }
}

#[test]
fn test_get_prow_artifact_url() -> Result<()> {
    let env = Env::new();
    let mut server = mockito::Server::new();
    let url = Url::parse(&server.url())?;
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

    let artifact_url = get_prow_artifact_url(&env, &url).expect("Artifact url");
    assert_eq!(
        artifact_url.as_str(),
        "https://artifacts.example.com/the-build/437/"
    );
    base_mock.assert();

    Ok(())
}

fn from_build_result(build: &ProwBuild, br: prow_build::BuildResult) -> Result<ProwBuild> {
    let url = build.url.join(&br.path)?;
    Ok(ProwBuild {
        url: url.clone(),
        uid: br.uid.0,
        job_name: build.job_name.clone(),
        project: "tbd".into(),
        pr: 0,
        storage_type: build.storage_type.clone(),
        storage_path: build.storage_path.clone(),
    })
}

pub fn discover_baselines(build: &ProwBuild, env: &Env) -> Result<Vec<Content>> {
    let client = prow_build::Client {
        client: env.client.clone(),
        api_url: build.url.clone(),
        storage_type: build.storage_type.as_ref().into(),
        storage_path: build.storage_path.as_ref().into(),
    };
    tracing::info!("Discovering baselines for {}", build);
    for baseline in prow_build::BuildIterator::new(&client, &build.job_name).take(200) {
        match baseline {
            Err(e) => return Err(anyhow::anyhow!("Failed to discover baseline: {}", e)),
            Ok(build_result) if build_result.result.as_ref() == "SUCCESS" => {
                return Ok(vec![Content::Prow(Box::new(from_build_result(
                    build,
                    build_result,
                )?))])
            }
            Ok(_) => {}
        }
    }
    Ok(vec![])
}

pub fn sources_iter(build: &ProwBuild, env: &Env) -> Box<dyn Iterator<Item = Result<SourceLoc>>> {
    match get_prow_artifact_url(env, &build.url) {
        Err(e) => Box::new(std::iter::once(Err(e))),
        Ok(url) => crate::httpdir_iter(&url, env),
    }
}
