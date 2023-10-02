// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use url::Url;

use crate::env::Env;
use crate::{Content, Source};

lazy_static::lazy_static! {
    static ref CACHE: logreduce_cache::Cache = logreduce_cache::Cache::new().expect("Cache");
}

#[tracing::instrument(level = "debug", skip(env))]
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

#[tracing::instrument(level = "debug", skip(env))]
pub fn url_open(env: &Env, prefix: usize, url: &Url) -> Result<crate::reader::DecompressReader> {
    tracing::debug!(url = url.as_str(), "Fetching url");
    crate::reader::from_url(env, prefix, url)
}

#[tracing::instrument(level = "debug", skip(env))]
pub fn httpdir_iter(url: &Url, env: &Env) -> Box<dyn Iterator<Item = Result<Source>>> {
    let base_len = url.as_str().trim_end_matches('/').len() + 1;
    // TODO: fix the httpdir cache to work with iterator
    let urls = match CACHE.httpdir_get(url) {
        Some(res) => res,
        None => httpdir::list_with_client(env.client.clone(), url.clone())
            .context("Can't list url")
            .and_then(|res| {
                CACHE.httpdir_add(url, &res)?;
                Ok(res)
            }),
    };
    match urls {
        Ok(urls) => Box::new(
            urls.into_iter()
                .map(move |u| Ok(Source::Remote(base_len, u))),
        ),
        Err(e) => Box::new(std::iter::once(Err(e))),
    }
}
