// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

#![warn(missing_docs)]

//! This library provides an Iterator to crawl the list of files available through Index of HTTP url.
//!
//! Here is an example usage:
//!
//! ```no_run
//! # fn main() -> std::io::Result<()> {
//! use url::Url;
//! let url = Url::parse("http://localhost/logs/").unwrap();
//! let files: Vec<Url> = httpdir::list(url)?;
//! # Ok(()) }
//! ```
//!
//! Or using the iterator:
//!
//! ```no_run
//! # fn main() -> std::io::Result<()> {
//! # use url::Url;
//! # let url = Url::parse("http://localhost/logs/").unwrap();
//! for file in httpdir::Crawler::new().walk(url) {
//!    let file: Url = file?;
//! }
//! # Ok(()) }
//! ```

use lazy_static::lazy_static;
use regex::Regex;
use std::io::Result;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use threadpool::ThreadPool;
use ureq::Agent;
use url::Url;

/// Helper function to walk a single url and list all the available files.
pub fn list(url: Url) -> Result<Vec<Url>> {
    Crawler::new().list(url)
}

/// The list function, but using the provided ureq client.
pub fn list_with_client(client: Agent, url: Url) -> Result<Vec<Url>> {
    Crawler::new_with_client(client).list(url)
}

/// Helper struct to prevent infinit loop.
struct Visitor {
    // todo: add unique host check
    // todo: add url hashset to check for unique url
    min_len: usize,
}

impl Visitor {
    fn new() -> Visitor {
        Visitor { min_len: 0 }
    }

    fn visit(&self, url: &Url) -> Option<Visitor> {
        let min_len = url.path().len();
        if min_len > self.min_len {
            Some(Visitor { min_len })
        } else {
            None
        }
    }
}

// The message type produced by a worker:
// - found a file url:  Some(url)
// - got an error:      Some(error)
// - finished the work: None
type Message = Option<Result<Url>>;

/// The state of the crawler, created calling `Crawler::new()`.
pub struct Crawler {
    client: Agent,
    // A worker pool.
    workers: ThreadPool,
    // The mpsc channel.
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

// Helper function to create an io Error
fn mk_error(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, msg)
}

impl Crawler {
    /// Initialize the Crawler state.
    pub fn new() -> Crawler {
        Crawler::new_with_client(Agent::new())
    }

    /// Initialize the Crawler state with the ureq client.
    pub fn new_with_client(client: Agent) -> Crawler {
        let workers = ThreadPool::new(4);
        let (tx, rx) = channel();
        Crawler {
            workers,
            client,
            tx,
            rx,
        }
    }

    /// A simple implementation to list all the available files.
    pub fn list(&self, url: Url) -> Result<Vec<Url>> {
        // Submit the initial task.
        self.start(url);
        // Wait for all the workers to complete.
        self.workers.join();
        // Collect the results.
        if self.workers.panic_count() > 0 {
            Err(mk_error("Crawler panicked!"))
        } else {
            self.rx.try_iter().flatten().collect()
        }
    }

    /// An iterator based implementation which works a bit differently to poll the results.
    pub fn walk(self, url: Url) -> impl Iterator<Item = Result<Url>> {
        // Submit the initial task.
        self.start(url);
        // Create the iterator state.
        CrawlerIter {
            workers: self.workers,
            abort: false,
            rx: self.rx,
        }
    }

    fn start(&self, url: Url) {
        // Here we pass all the requirements by reference to avoid lifetime issues.
        Crawler::process(&Visitor::new(), &self.client, &self.workers, &self.tx, url);
    }

    // Helper function to handle a single url.
    fn process(
        visitor: &Visitor,
        client: &Agent,
        pool: &ThreadPool,
        tx: &Sender<Message>,
        url: Url,
    ) {
        let base_url = url.as_str().to_string();
        if let Some(visitor) = visitor.visit(&url) {
            // Increase reference counts.
            let tx = tx.clone();
            let sub_pool = pool.clone();
            let client = client.clone();

            // Submit the work.
            pool.execute(move || match http_list(&client, &url) {
                // We decoded some urls.
                Ok(urls) => {
                    for url in urls {
                        if url.path().ends_with("/etc/") || !url.as_str().starts_with(&base_url) {
                            // Special case to avoid system config directory
                            continue;
                        } else if let Some(url) = path_dir(&url) {
                            // Recursively call the handler on sub directory.
                            Crawler::process(&visitor, &client, &sub_pool, &tx, url)
                        } else {
                            // Send file location to the mpsc channel.
                            tx.send(Some(Ok(url))).unwrap()
                        }
                    }
                    // Indicate we are done.
                    tx.send(None).unwrap()
                }

                // An error happened, propagates it.
                Err(e) => tx.send(Some(Err(e))).unwrap(),
            });
        }
    }
}

impl Default for Crawler {
    fn default() -> Self {
        Self::new()
    }
}

// The state of the iterator.
struct CrawlerIter {
    workers: ThreadPool,
    abort: bool,
    rx: Receiver<Message>,
}

impl CrawlerIter {
    // We are done when all the work is completed: no worker are active or queued.
    fn is_done(&self) -> bool {
        self.abort
            || (self.workers.active_count() + self.workers.queued_count() == 0
                // ensure that when there is a panic, we propagate the error.
                && self.workers.panic_count() == 0)
    }
}

impl Iterator for CrawlerIter {
    type Item = Result<Url>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rx.try_recv() {
            // A new url was found, we return it.
            Ok(Some(r)) => Some(r),

            // A worker finished it's job, we keep on iterating.
            Ok(None) => self.next(),

            // No messages are available and all the workers are done, we stop the iterator.
            Err(_) if self.is_done() => None,

            // There was a panic
            Err(_) if self.workers.panic_count() > 0 => {
                self.abort = true;
                Some(Err(mk_error("Crawler panicked!")))
            }

            // The workers are still active, we can block recv to wait for the next message.
            Err(_) => match self.rx.recv_timeout(Duration::from_secs(1)) {
                // A new url was found, we can return it.
                Ok(Some(r)) => Some(r),

                // A worker completed it's task, we keep on iterating.
                _ => self.next(),
            },
        }
    }
}

fn path_dir(url: &Url) -> Option<Url> {
    if url.path().ends_with('/') {
        Some(url.clone())
    } else if url.path().ends_with("/index.html") {
        let mut new_url = url.clone();
        let new_len = url.path().len() - 10;
        new_url.set_path(&url.path()[..new_len]);
        Some(new_url)
    } else {
        None
    }
}

/// List the files and directories of a single url.
pub fn http_list(client: &Agent, url: &Url) -> Result<Vec<Url>> {
    // dbg!(&url);
    match client.request_url("GET", url).call() {
        Ok(resp) => {
            let url =
                Url::parse(resp.get_url()).map_err(|e| mk_error(&format!("Bad url {}", e)))?;
            let body = resp.into_string()?;
            parse_index_of(url, &body)
        }
        Err(e) => Err(mk_error(&format!("Request failed {}", e))),
    }
}

fn parse_index_of(base_url: Url, page: &str) -> Result<Vec<Url>> {
    // TODO check for title and support different types of indexes
    lazy_static! {
        static ref RE: Regex = Regex::new(r#"<a href="(\./)*([\\/a-zA-Z0-9][^"]+)""#).unwrap();
    }

    RE.captures_iter(page)
        .map(|c| c.get(2).unwrap().as_str())
        // .map(|link| dbg!(link))
        .map(|link| {
            base_url
                .join(link)
                .map_err(|e| mk_error(&format!("Invalid link {}", e)))
        })
        .collect()
}

#[test]
fn test_main_httpdir() {
    use itertools::Itertools;
    println!("Starting test_httpdir");
    let mut server = mockito::Server::new();

    let root = "/logs/98/24398/5/check/dhall-diff/23b9eed/";
    let url = Url::parse(&server.url()).unwrap().join(root);
    println!("Adding base mock");
    let base_mock = server.mock("GET", root)
        .with_body(
            r#"
<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 3.2 Final//EN">
<html>
 <head>
  <title>Index of /logs/98/24398/5/check/dhall-diff/23b9eed</title>
 </head>
 <body>
<h1>Index of /logs/98/24398/5/check/dhall-diff/23b9eed</h1>
  <table>
   <tr><th valign="top"><img src="/icons/blank.gif" alt="[ICO]"></th><th><a href="?C=N;O=D">Name</a></th><th><a href="?C=M;O=A">Last modified</a></th><th><a href="?C=S;O=A">Size</a></th></tr>
   <tr><th colspan="4"><hr></th></tr>
<tr><td valign="top"><img src="/icons/back.gif" alt="[PARENTDIR]"></td><td><a href="/logs/98/24398/5/check/dhall-diff/">Parent Directory</a></td><td>&nbsp;</td><td align="right">  - </td></tr>
<tr><td valign="top"><img src="/icons/compressed.gif" alt="[   ]"></td><td><a href="job-output.json.gz">job-output.json.gz</a></td><td align="right">2022-03-23 17:33  </td><td align="right">7.0K</td></tr>
<tr><td valign="top"><img src="/icons/compressed.gif" alt="[   ]"></td><td><a href="job-output.txt.gz">job-output.txt.gz</a></td><td align="right">2022-03-23 17:33  </td><td align="right">4.7K</td></tr>
<tr><td valign="top"><img src="/icons/folder.gif" alt="[DIR]"></td><td><a href="zuul-info/">zuul-info/</a></td><td align="right">2022-03-23 17:31  </td><td align="right">  - </td></tr>
<tr><td valign="top"><img src="/icons/unknown.gif" alt="[   ]"></td><td><a href="zuul-manifest.json">zuul-manifest.json</a></td><td align="right">2022-03-23 17:33  </td><td align="right">478 </td></tr>
   <tr><th colspan="4"><hr></th></tr>
</table>
</body></html>
"#).expect(2).create(); // we expect 2 because we run the crawler twice

    println!("Adding info mock");
    let info_mock = server.mock("GET", &*format!("{}zuul-info/", root)).with_body(
        r#"
<tr><td valign="top"><img src="/icons/back.gif" alt="[PARENTDIR]"></td><td><a href="/logs/98/24398/5/check/dhall-diff/23b9eed/">Parent Directory</a></td><td>&nbsp;</td><td align="right">  - </td></tr>
<tr><td valign="top"><img src="/icons/text.gif" alt="[TXT]"></td><td><a href="inventory.yaml">inventory.yaml</a></td><td align="right">2022-03-23 17:31  </td><td align="right">817 </td></tr>
"#).expect(2).create();

    println!("Adding catch_all mock");
    let catch_all = server
        .mock("GET", mockito::Matcher::Any)
        .with_body("oops")
        .expect(0)
        .create();

    println!("Calling httpdir::list");
    let res = list(url.clone().unwrap())
        .unwrap()
        .into_iter()
        .sorted()
        .collect::<Vec<_>>();

    dbg!(&res);
    assert!(res.len() == 4);

    let iter_res = Crawler::new()
        .walk(url.unwrap())
        .map(|r| r.unwrap())
        .sorted()
        .collect::<Vec<_>>();
    assert_eq!(res, iter_res);

    catch_all.assert();
    info_mock.assert();
    base_mock.assert();
}

#[test]
fn test_prow_httpdir() {
    let mut server = mockito::Server::new();

    let root = "/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/";
    let url = Url::parse(&server.url()).unwrap().join(root);
    let base_mock = server.mock("GET", root)
        .with_body(
            r#"
    <!doctype html>
    <html>
    <head>
        <link rel="stylesheet" type="text/css" href="/styles/style.css">
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>GCS browser: origin-ci-test</title>
    </head>
    <body>
    <header>
        <h1>origin-ci-test</h1>
        <h3>/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/</h3>
    </header>
    <ul class="resource-grid">

    <li class="pure-g">
        <div class="pure-u-2-5 grid-head">Name</div>
        <div class="pure-u-1-5 grid-head">Size</div>
        <div class="pure-u-2-5 grid-head">Modified</div>
    </li>

    <li class="pure-g grid-row">
        <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/"><img src="/icons/back.png"> ..</a></div>
        <div class="pure-u-1-5">-</div>
        <div class="pure-u-2-5">-</div>
    </li>

    <li class="pure-g grid-row">
        <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/artifacts/"><img src="/icons/dir.png"> artifacts/</a></div>
        <div class="pure-u-1-5">-</div>
        <div class="pure-u-2-5">-</div>
    </li>

    <li class="pure-g grid-row">
        <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/build-log.txt"><img src="/icons/file.png"> build-log.txt</a></div>
        <div class="pure-u-1-5">1627</div>
        <div class="pure-u-2-5">Thu, 10 Aug 2023 13:14:15 UTC</div>
    </li>
    </ul>
    <details>
        <summary style="display: list-item; padding-left: 1em">Download</summary>
        <div style="padding: 1em">
            You can download this directory by running the following <a href="https://cloud.google.com/storage/docs/gsutil">gsutil</a> command:
            <pre>gsutil -m cp -r gs://origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/artifacts/ .</pre>
        </div>
    </details>
    </body></html>
"#).expect(1).create();
    let artifacts_mock = server.mock("GET", &*format!("{}artifacts/", root)).with_body(
        r#"
    <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/"><img src="/icons/back.png"> ..</a></div>
    <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/artifacts/build-logs/"><img src="/icons/dir.png"> build-logs/</a></div>
    <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/artifacts/ci-operator-step-graph.json"><img src="/icons/file.png"> ci-operator-step-graph.json</a></div>
"#).expect(1).create();
    let builds_mock = server.mock("GET", &*format!("{}artifacts/build-logs/", root)).with_body(
        r#"
    <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/artifacts/"><img src="/icons/back.png"> ..</a></div>
    <div class="pure-u-2-5"><a href="/gcs/origin-ci-test/pr-logs/pull/openstack-k8s-operators_ci-framework/437/pull-ci-openstack-k8s-operators-ci-framework-main-ansible-test/1689624623181729792/artifacts/build-logs/ci-framework-image.log"><img src="/icons/file.png"> ci-framework-image.log</a></div>
"#).expect(1).create();
    let catch_all = server
        .mock("GET", mockito::Matcher::Any)
        .with_body("oops")
        .expect(0)
        .create();

    let res = list(url.clone().unwrap()).unwrap();

    dbg!(&res);
    assert_eq!(res.len(), 3);

    catch_all.assert();
    base_mock.assert();
    builds_mock.assert();
    artifacts_mock.assert();
}

#[test]
fn test_targro_httpdir() {
    let base = Url::parse("http://localhost/job/").unwrap();
    let urls = parse_index_of(base, r#"
      <tr>
        <td class="name up"><a href="../">..</a></td>
        <td></td>
        <td></td>
      </tr>
      <tr>
        <td colspan="3"><hr /></td>
      </tr>
      <tr class="entry">
        <td class="name file">
          <a href="./tempest-results-barbican.1.html"
            >tempest-results-barbican.1.html</a
          >
        </td>
        <td>26-Sep-2023 17:35</td>
        <td class="size">267665</td>
      </tr>
      <tr class="entry">
        <td class="name dir"><a href="./cephstorage-0/">cephstorage-0/</a></td><td>26-Sep-2023 17:47</td><td class="size">56</td></tr>
"#).unwrap();
    assert_eq!(
        urls.iter().map(|u| u.as_str()).collect::<Vec<&str>>(),
        vec![
            "http://localhost/job/tempest-results-barbican.1.html",
            "http://localhost/job/cephstorage-0/"
        ]
    )
}
