// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]

//! This library provides an Iterator to crawl build results from [prow](https://prow.k8s.io/).
//!
//! Here is an example usage:
//!
//! ```no_run
//! # fn main() {
//! let client = prow_build::Client {
//!   client: ureq::Agent::new(),
//!   api_url: url::Url::parse("https://prow.ci.openshift.org/").unwrap(),
//!   storage_type: "gs".into(),
//!   storage_path: "origin-ci-test".into(),
//! };
//! let max_result = 42;
//! for build in prow_build::BuildIterator::new(&client, "my-job").take(max_result) {
//!   println!("{:#?}", build);
//! }
//! # }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::BufRead;
use thiserror::Error;
use url::Url;

/// The prow client.
pub struct Client {
    /// The HTTP client.
    pub client: ureq::Agent,
    /// The prow api url.
    pub api_url: Url,
    /// The build storage type.
    pub storage_type: StorageType,
    /// The build storage path.
    pub storage_path: StoragePath,
}

/// The prow error.
#[derive(Error, Debug)]
pub enum Error {
    /// The provided url is not usable.
    #[error("bad api url: {0}")]
    BadUrl(#[from] url::ParseError),

    /// The api reply contained an unexpected error.
    #[error("bad api reply: {0}")]
    BadReply(#[from] std::io::Error),

    /// The api query failed.
    #[error("bad api query: {0}")]
    BadQuery(#[from] Box<ureq::Error>),

    /// The api response was not usable.
    #[error("Api response didn't contain builds")]
    BadResponse,

    /// The api response couldn't be decoded.
    #[error("Api response decoding error: {0}")]
    BadBuild(#[from] serde_json::Error),
}

/// A build id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProwID(String);

/// The storage type, e.g. "gs"
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StorageType(String);

/// The storage path, e.g. "origin-ci-test"
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StoragePath(String);

impl From<&str> for ProwID {
    /// Converts to this type from the input type.
    #[inline]
    fn from(s: &str) -> ProwID {
        ProwID(s.to_owned())
    }
}

impl From<ProwID> for String {
    /// Converts to this type from the input type.
    #[inline]
    fn from(pid: ProwID) -> String {
        pid.0
    }
}

impl From<&str> for StorageType {
    /// Converts to this type from the input type.
    #[inline]
    fn from(s: &str) -> StorageType {
        StorageType(s.to_owned())
    }
}

impl From<&str> for StoragePath {
    /// Converts to this type from the input type.
    #[inline]
    fn from(s: &str) -> StoragePath {
        StoragePath(s.to_owned())
    }
}

/// A build result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildResult {
    /// The build id.
    #[serde(rename = "ID")]
    pub uid: ProwID,
    /// The spyglass path.
    #[serde(rename = "SpyglassLink")]
    pub path: String,
    /// The build result.
    #[serde(rename = "Result")]
    pub result: String,
    /// The build started date.
    #[serde(rename = "Started")]
    pub started: DateTime<Utc>,
    /// The build duration.
    #[serde(rename = "Duration")]
    pub duration: usize,
}

/// The iterator state.
pub struct BuildIterator<'a> {
    client: &'a Client,
    job_name: &'a str,
    skip: Option<ProwID>,
    buffer: VecDeque<BuildResult>,
    done: bool,
}

impl<'a> BuildIterator<'a> {
    /// Create a new iterator.
    pub fn new(client: &'a Client, job_name: &'a str) -> Self {
        BuildIterator {
            client,
            job_name,
            skip: None,
            buffer: VecDeque::new(),
            done: false,
        }
    }
}

impl Iterator for BuildIterator<'_> {
    type Item = Result<BuildResult, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else if let Some(res) = self.buffer.pop_front() {
            Some(Ok(res))
        } else {
            match get_prow_job_history(self.client, self.job_name, &self.skip) {
                Ok(res) => self.handle_new_builds(res),
                Err(err) => {
                    self.done = true;
                    Some(Err(err))
                }
            }
        }
    }
}

impl BuildIterator<'_> {
    fn handle_new_builds(
        &mut self,
        builds: Vec<BuildResult>,
    ) -> Option<Result<BuildResult, Error>> {
        if let Some(last) = builds.last() {
            // We have some builds
            self.skip = Some(last.uid.clone());
            self.buffer = builds.into();
            self.next()
        } else {
            self.done = true;
            None
        }
    }
}

// It doesn't seem like prow provides a REST API. Thus this function decodes the builds embeded as JSON object inside the html page.
/// The low-level function to query a single page.
pub fn get_prow_job_history(
    client: &Client,
    job_name: &str,
    after: &Option<ProwID>,
) -> Result<Vec<BuildResult>, Error> {
    let mut api_url = client.api_url.clone();
    api_url.set_path(&format!(
        "/job-history/{}/{}/pr-logs/directory/{}",
        client.storage_type.0, client.storage_path.0, job_name
    ));
    if let Some(after) = after {
        api_url.set_query(Some(&format!("buildId={}", after.0)))
    }
    tracing::debug!(url = api_url.as_str(), "Querying prow job history");
    let reader = client
        .client
        .request_url("GET", &api_url)
        .call()
        .map_err(|e| Error::BadQuery(Box::new(e)))?
        .into_reader();
    let js_objs = std::io::BufReader::new(reader).lines().find(|le| {
        le.as_ref()
            .is_ok_and(|l| l.trim().starts_with("var allBuilds = "))
    });
    match js_objs {
        None => Err(Error::BadResponse),
        Some(Err(err)) => Err(Error::BadReply(err)),
        Some(Ok(js_objs)) => {
            let start_pos = js_objs.find('=').unwrap_or(0) + 1;
            serde_json::de::from_str(js_objs.get(start_pos..).unwrap_or("").trim_end_matches(';'))
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
        .expect(2) // We call this route twice
        .create();

    let page_1_path = format!("{}?buildId={}", path, "1691081796252340224");
    let page_1 = server.mock("GET", &*page_1_path).with_body(
        r#"
  var allBuilds = [{"SpyglassLink":"/view/gs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/444/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1691081796252340224","ID":"1691081796252340225","Started":"2023-08-14T13:38:24Z","Duration":241000000000,"Result":"SUCCESS","Refs":{"org":"openstack-k8s-operators","repo":"ci-framework","repo_link":"https://github.com/openstack-k8s-operators/ci-framework","base_ref":"main","base_sha":"fefd236c551a4ce7a2bd5a582bb6b3b23a86b3b0","base_link":"https://github.com/openstack-k8s-operators/ci-framework/commit/fefd236c551a4ce7a2bd5a582bb6b3b23a86b3b0","pulls":[{"number":444,"author":"raukadah","sha":"da139a175eaabe4052899b1beda10798d34d62c8","title":"Added taskfiles to collect edpm logs from edpm vms","head_ref":"edpm_logging","link":"https://github.com/openstack-k8s-operators/ci-framework/pull/444","commit_link":"https://github.com/openstack-k8s-operators/ci-framework/pull/444/commits/da139a175eaabe4052899b1beda10798d34d62c8","author_link":"https://github.com/raukadah"}]}}]
"#,
).expect(1).create();

    let page_2_path = format!("{}?buildId={}", path, "1691081796252340225");
    let page_2 = server
        .mock("GET", &*page_2_path)
        .with_body(r#"  var allBuilds = []"#)
        .expect(1)
        .create();

    let client = Client {
        client: ureq::Agent::new(),
        api_url: Url::parse(&server.url()).unwrap(),
        storage_type: "gs".into(),
        storage_path: "origin-ci-test".into(),
    };

    let builds = get_prow_job_history(&client, job_name, &None).unwrap();
    dbg!(&builds);
    assert_eq!(builds.len(), 1);

    let builds2 = BuildIterator::new(&client, job_name).collect::<Vec<_>>();
    assert_eq!(builds2.len(), 2);
    base_mock.assert();
    page_1.assert();
    page_2.assert()
}
