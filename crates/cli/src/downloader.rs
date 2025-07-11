// Copyright (C) 2025 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use logjuicer_model::{
    content_from_input, content_get_sources, env::EnvConfig, urls::url_open, Input, Source,
};
use std::path::PathBuf;

pub fn download(base_env: EnvConfig, dest: PathBuf, input: Input) -> Result<()> {
    let content = content_from_input(&base_env.gl, input)?;
    let sources = content_get_sources(&base_env.get_target_env(&content), &content)?;
    for source in sources {
        if let Source::Remote(base, url) = source {
            let path = dest.join(&url.as_str()[base..]);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
                println!("Fetching {} to {}", url.as_str(), path.display());
                let mut inp = url_open(&base_env.gl, &url)?;
                let mut out = std::fs::File::create(path)?;
                std::io::copy(&mut inp, &mut out)?;
            }
        }
    }
    Ok(())
}
