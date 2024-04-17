// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

use logjuicer_model::ModelF;
use logjuicer_report::model_row::ContentID;

fn model_path(content_id: &ContentID) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    format!(
        "data/model_{:X}.gz",
        Sha256::digest(content_id.0.as_bytes())
    )
    .into()
}

pub fn load_model(content_id: &ContentID) -> Result<ModelF, String> {
    ModelF::load(&model_path(content_id)).map_err(|e| format!("Reading the model failed {:?}", e))
}

pub fn save_model(content_id: &ContentID, model: &ModelF) -> Result<(), String> {
    model
        .save(&model_path(content_id))
        .map_err(|e| format!("Writing the model failed {:?}", e))
}

pub fn delete_model(content_id: &ContentID) {
    let path = model_path(content_id);
    let _ = std::fs::remove_file(path);
}
