// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::BufRead;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum Error {
    #[error("bad api url")]
    BadUrl(#[from] url::ParseError),

    #[error("bad api reply")]
    BadReply(#[from] std::io::Error),

    #[error("bad api query")]
    BadQuery(#[from] reqwest::Error),

    #[error("Api response didn't contain builds")]
    BadResponse,

    #[error("Api response decoding error")]
    BadBuild(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProwID(String);

#[derive(Clone, Debug, PartialEq)]
pub struct StorageType(String);

#[derive(Clone, Debug, PartialEq)]
pub struct StoragePath(String);

impl From<&str> for StorageType {
    #[inline]
    fn from(s: &str) -> StorageType {
        StorageType(s.to_owned())
    }
}

impl From<&str> for StoragePath {
    #[inline]
    fn from(s: &str) -> StoragePath {
        StoragePath(s.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildResult {
    #[serde(rename = "ID")]
    pub uid: ProwID,
    #[serde(rename = "SpyglassLink")]
    pub path: String,
    #[serde(rename = "Result")]
    pub result: String,
    #[serde(rename = "Started")]
    pub started: DateTime<Utc>,
    #[serde(rename = "Duration")]
    pub duration: usize,
}

// It doesn't seem like prow provides a REST API. Thus this function decodes the builds embeded as JSON object inside the html page.
pub fn get_prow_job_history(
    client: reqwest::blocking::Client,
    api_url: &Url,
    storage_type: StorageType,
    storage_path: StoragePath,
    job_name: &str,
    after: Option<ProwID>,
) -> Result<Vec<BuildResult>, Error> {
    let mut api_url = api_url.clone();
    api_url.set_path(&format!(
        "/job-history/{}/{}/pr-logs/directory/{}",
        storage_type.0, storage_path.0, job_name
    ));
    if let Some(after) = after {
        api_url.set_query(Some(&format!("buildId={}", after.0)))
    }
    dbg!(&api_url.as_str());
    let reader = client.get(api_url).send().map_err(Error::BadQuery)?;
    let js_objs = std::io::BufReader::new(reader).lines().find(|le| {
        le.as_ref()
            .is_ok_and(|l| l.trim().starts_with("var allBuilds = "))
    });
    match js_objs {
        None => Err(Error::BadResponse),
        Some(Err(err)) => Err(Error::BadReply(err)),
        Some(Ok(js_objs)) => {
            let start_pos = js_objs.find('=').unwrap_or(0) + 1;
            serde_json::de::from_str(js_objs.get(start_pos..).unwrap_or(""))
                .map_err(Error::BadBuild)
        }
    }
}

#[test]
fn test_get_prow_job_history() {
    let mut server = mockito::Server::new();
    let job_name = "tasty-job";
    let path: &str = &format!(
        "/job-history/gs/origin-ci-test/pr-logs/directory/{}",
        job_name
    );
    let base_mock = server
        .mock("GET", path)
        .with_body(
            r#"
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Job History: pr-logs/directory/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test</title>

<script type="text/javascript">
  var allBuilds = [{"SpyglassLink":"/view/gs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/444/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1691081796252340224","ID":"1691081796252340224","Started":"2023-08-14T13:38:24Z","Duration":241000000000,"Result":"SUCCESS","Refs":{"org":"openstack-k8s-operators","repo":"ci-framework","repo_link":"https://github.com/openstack-k8s-operators/ci-framework","base_ref":"main","base_sha":"fefd236c551a4ce7a2bd5a582bb6b3b23a86b3b0","base_link":"https://github.com/openstack-k8s-operators/ci-framework/commit/fefd236c551a4ce7a2bd5a582bb6b3b23a86b3b0","pulls":[{"number":444,"author":"raukadah","sha":"da139a175eaabe4052899b1beda10798d34d62c8","title":"Added taskfiles to collect edpm logs from edpm vms","head_ref":"edpm_logging","link":"https://github.com/openstack-k8s-operators/ci-framework/pull/444","commit_link":"https://github.com/openstack-k8s-operators/ci-framework/pull/444/commits/da139a175eaabe4052899b1beda10798d34d62c8","author_link":"https://github.com/raukadah"}]}}]
</script>
<html>
"#,
        )
        .expect(1)
        .create();

    let client = reqwest::blocking::Client::new();

    let url = Url::parse(&server.url()).unwrap();
    let builds = get_prow_job_history(
        client,
        &url,
        "gs".into(),
        "origin-ci-test".into(),
        job_name,
        None,
    )
    .unwrap();
    dbg!(&builds);
    assert_eq!(builds.len(), 1);
    base_mock.assert();
}
