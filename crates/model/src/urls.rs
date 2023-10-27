// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use url::Url;

use crate::env::Env;
use crate::{Content, Source};

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
    let maybe_cached = if let Some(cache) = &env.cache {
        cache.httpdir_get(url)
    } else {
        None
    };
    let urls = if let Some(cached) = maybe_cached {
        cached
    } else {
        let urls = httpdir::list_with_client(env.client.clone(), url.clone())
            .into_iter()
            // Convert httpdir error into cachable error
            .map(|url_result| url_result.map_err(|e| format!("{:?}", e).into()))
            .collect::<Vec<logjuicer_cache::UrlResult>>();
        if let Some(cache) = &env.cache {
            cache.httpdir_add(url, &urls).map(|()| urls)
        } else {
            Ok(urls)
        }
    };

    match urls {
        Ok(urls) => Box::new(urls.into_iter().map(move |url_result| {
            url_result
                .map_err(anyhow::Error::msg)
                .map(|url| Source::Remote(base_len, url))
        })),
        Err(e) => Box::new(std::iter::once(Err(e))),
    }
}
