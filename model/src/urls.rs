// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use url::Url;

use crate::{Content, Source};

lazy_static::lazy_static! {
    static ref CACHE: logreduce_cache::Cache = logreduce_cache::Cache::new().expect("Cache");
}

impl Content {
    #[tracing::instrument]
    pub fn from_url(url: Url) -> Result<Content> {
        if !url.has_authority() {
            Err(anyhow::anyhow!("Bad url {}", url))
        } else if url.as_str().ends_with('/') {
            Ok(Content::Directory(Source::Remote(None, url)))
        } else {
            Ok(Content::File(Source::Remote(None, url)))
        }
    }
}

impl Source {
    #[tracing::instrument]
    pub fn url_open(url: &Url) -> Result<crate::reader::DecompressReader> {
        crate::reader::from_url(url)
    }

    #[tracing::instrument]
    pub fn httpdir_iter(url: &Url) -> Box<dyn Iterator<Item = Result<Source>>> {
        let base = url.clone();
        // TODO: fix the httpdir cache to work with iterator
        let urls = match CACHE.httpdir_get(url) {
            Some(res) => res,
            None => httpdir::list(url.clone())
                .context("Can't list url")
                .and_then(|res| {
                    CACHE.httpdir_add(url, &res)?;
                    Ok(res)
                }),
        };
        match urls {
            Ok(urls) => Box::new(urls.into_iter().map(move |u| Ok(Source::Remote(Some(base.clone()), u)))),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }
}
