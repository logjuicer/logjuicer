// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a global environment.

pub struct Env {
    pub cache: logreduce_cache::Cache,
    pub client: ureq::Agent,
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
            client: new_agent(),
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

fn new_agent() -> ureq::Agent {
    new_agent_safe().expect("ureq agent creation failed")
}

fn http_proxy() -> Result<String, std::env::VarError> {
    std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
}

fn tls_ca_bundle() -> Option<std::ffi::OsString> {
    std::env::var_os("LOGREDUCE_CA_BUNDLE")
        .or_else(|| std::env::var_os("REQUESTS_CA_BUNDLE"))
        .or_else(|| std::env::var_os("CURL_CA_BUNDLE"))
}

// Copied from https://github.com/PyO3/maturin/blob/23158969c97418b07a3c4d31282d220ec08c3c10/src/upload.rs#L395-L418
fn new_agent_safe() -> Result<ureq::Agent, std::io::Error> {
    use std::sync::Arc;

    let mut builder = ureq::builder();
    if let Ok(proxy) = http_proxy() {
        let proxy = ureq::Proxy::new(proxy)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        builder = builder.proxy(proxy);
    };
    if let Some(ca_bundle) = tls_ca_bundle() {
        let mut reader = std::io::BufReader::new(std::fs::File::open(ca_bundle)?);
        let certs = rustls_pemfile::certs(&mut reader)?;
        let mut root_certs = rustls::RootCertStore::empty();
        root_certs.add_parsable_certificates(&certs);
        let client_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_certs)
            .with_no_client_auth();
        Ok(builder.tls_config(Arc::new(client_config)).build())
    } else {
        Ok(builder.build())
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
