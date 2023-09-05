// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a global environment.

pub struct Env {
    pub cache: logreduce_cache::Cache,
    pub client: reqwest::blocking::Client,
    pub use_cache: bool,
    pub output: OutputMode,
}

impl Env {
    pub fn new() -> Env {
        Env::new_with_output(OutputMode::Debug)
    }

    pub fn new_with_output(output: OutputMode) -> Env {
        Env {
            cache: logreduce_cache::Cache::new().expect("Cache"),
            client: reqwest::blocking::Client::builder()
                .danger_accept_invalid_certs(std::env::var("LOGREDUCE_SSL_NO_VERIFY").is_ok())
                .build()
                .expect("Client"),
            use_cache: std::env::var("LOGREDUCE_CACHE").is_ok(),
            output,
        }
    }

    /// Helper function to debug
    pub fn debug_or_progress(&self, msg: &str) {
        match self.output {
            OutputMode::FastTerminal => print!("\r\x1b[1;33m[+]\x1b[0m {}", msg),
            OutputMode::Debug => tracing::debug!("{}", msg),
            OutputMode::Quiet => {}
        }
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub enum OutputMode {
    // Print every steps
    Debug,
    // Print progress using \r
    FastTerminal,
    // Do not print progress, only errors
    Quiet,
}

impl OutputMode {
    pub fn inlined(&self) -> bool {
        matches!(self, OutputMode::FastTerminal)
    }
}
