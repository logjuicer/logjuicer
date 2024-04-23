// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use logjuicer_model::ModelF;
use logjuicer_report::model_row::ContentID;

fn model_path(storage_dir: &str, content_id: &ContentID) -> PathBuf {
    use sha2::{Digest, Sha256};
    format!(
        "{storage_dir}/models/model_{:X}.gz",
        Sha256::digest(content_id.0.as_bytes())
    )
    .into()
}

#[cfg(test)]
pub fn test_model_path(storage_dir: &str, content_id: &ContentID) -> PathBuf {
    model_path(storage_dir, content_id)
}

pub fn load_model(storage_dir: &str, content_id: &ContentID) -> Result<ModelF, String> {
    ModelF::load(&model_path(storage_dir, content_id))
        .map_err(|e| format!("Reading the model failed {:?}", e))
}

pub fn save_model(
    storage_dir: &str,
    content_id: &ContentID,
    model: &ModelF,
) -> Result<PathBuf, String> {
    let path = model_path(storage_dir, content_id);
    model
        .save(&path)
        .map_err(|e| format!("Writing the model failed {:?}", e))?;
    Ok(path)
}

pub fn delete_model(storage_dir: &str, content_id: &ContentID) {
    let path = model_path(storage_dir, content_id);
    let _ = std::fs::remove_file(path);
}
