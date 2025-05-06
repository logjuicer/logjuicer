// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use url::Url;

use crate::env::Env;
use crate::{Content, Source};

pub fn content_from_url(env: &Env, url: Url) -> Result<Content> {
    if !url.has_authority() {
        Err(anyhow::anyhow!("Bad url {}", url))
    } else if let Some(content) = crate::zuul::content_from_zuul_url(env, &url) {
        content
    } else if let Some(content) = crate::prow::content_from_prow_url(&url) {
        content
    } else if url.as_str().ends_with('/') {
        Ok(Content::Directory(Source::Remote(0, url)))
    } else {
        Ok(Content::File(Source::Remote(0, url)))
    }
}

#[tracing::instrument(level = "debug", skip_all, fields(url = url.as_str()))]
pub fn url_open(env: &Env, url: &Url) -> Result<crate::reader::DecompressReader> {
    tracing::debug!(url = url.as_str(), "Requesting url");
    crate::reader::get_url(env, url)
}

#[tracing::instrument(level = "debug", skip_all, fields(url = url.as_str()))]
pub fn httpdir_iter(url: &Url, env: &Env) -> Box<dyn Iterator<Item = Result<Source>>> {
    let base_len = url.as_str().trim_end_matches('/').len() + 1;
    let req_max = 2500;
    Box::new(
        httpdir::list_with_client(env.client.clone(), env.auth.clone(), req_max, url.clone())
            .into_iter()
            // Convert httpdir result into source item
            .map(move |url_result| {
                url_result
                    .map_err(anyhow::Error::msg)
                    .map(|url| Source::Remote(base_len, url))
            }),
    )
}
