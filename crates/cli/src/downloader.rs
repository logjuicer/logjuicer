// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use logjuicer_model::{
    content_from_input, content_get_sources,
    env::{Env, EnvConfig},
    urls::url_open,
    Input, SourceLoc,
};
use std::path::PathBuf;
use threadpool::ThreadPool;

fn download_file(env: &Env, url: url::Url, path: PathBuf) -> Result<()> {
    println!("Fetching {} to {}", url.as_str(), path.display());
    let mut inp = url_open(env, &url)?;
    let mut out = std::fs::File::create(path)?;
    std::io::copy(&mut inp, &mut out)?;
    Ok(())
}

pub fn download(base_env: EnvConfig, dest: PathBuf, input: Input) -> Result<()> {
    let content = content_from_input(&base_env.gl, input)?;
    let sources = content_get_sources(&base_env.get_target_env(&content), &content)?;
    let pool = ThreadPool::new(5);
    for source in sources {
        if let SourceLoc::Remote(base, url) = source {
            let path = dest.join(&url.as_str()[base..]);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
                let env = base_env.gl.clone();
                pool.execute(move || download_file(&env, url, path).expect("Download"));
            }
        }
    }
    pool.join();
    Ok(())
}
