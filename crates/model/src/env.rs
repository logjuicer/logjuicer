// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a global environment.

use crate::{
    config::{Config, TargetConfig},
    unordered::KnownLines,
};
use anyhow::Result;
use logjuicer_report::Content;

pub struct Env {
    pub cache: Option<logjuicer_cache::Cache>,
    pub client: ureq::Agent,
    pub output: OutputMode,
}

impl Env {
    pub fn new() -> Self {
        Env::new_with_settings(OutputMode::Debug)
    }

    pub fn new_with_settings(output: OutputMode) -> Self {
        let cache = if std::env::var("LOGJUICER_CACHE").is_ok() {
            Some(logjuicer_cache::Cache::new().expect("Cache"))
        } else {
            None
        };
        Env {
            cache,
            client: new_agent(),
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

/// The environment to process a target
pub struct TargetEnv<'a> {
    pub config: &'a TargetConfig,
    pub gl: &'a Env,
}

impl<'a> TargetEnv<'a> {
    pub fn new_skip_lines(&self) -> Option<KnownLines> {
        self.config.new_skip_lines()
    }
}

/// The global environment
pub struct EnvConfig {
    pub gl: Env,
    pub config: Config,
}

impl EnvConfig {
    pub fn new() -> Self {
        Self::new_with_settings(None, OutputMode::Debug).unwrap()
    }

    pub fn new_with_settings(
        config: Option<std::path::PathBuf>,
        output: OutputMode,
    ) -> Result<Self> {
        let gl = Env::new_with_settings(output);
        let config = config
            .map(Config::from_path)
            .unwrap_or_else(|| Ok(Config::default()))?;
        Ok(Self { gl, config })
    }

    pub fn get_target_env<'a>(&'a self, target: &Content) -> TargetEnv<'a> {
        TargetEnv {
            config: self.config.get_target_config(target),
            gl: &self.gl,
        }
    }

    /// Helper function to debug
    pub fn debug_or_progress(&self, msg: &str) {
        self.gl.debug_or_progress(msg)
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

fn default_ca_bundle() -> Option<std::ffi::OsString> {
    let path = std::path::Path::new("/etc/pki/tls/certs/ca-bundle.crt");
    if path.exists() {
        Some(path.into())
    } else {
        None
    }
}

fn tls_ca_bundle() -> Option<std::ffi::OsString> {
    std::env::var_os("LOGJUICER_CA_BUNDLE")
        .or_else(|| std::env::var_os("REQUESTS_CA_BUNDLE"))
        .or_else(|| std::env::var_os("CURL_CA_BUNDLE"))
        .or_else(default_ca_bundle)
}

fn default_ca_extra() -> Option<std::ffi::OsString> {
    let path = std::path::Path::new("/etc/pki/tls/certs/ca-extra.crt");
    if path.exists() {
        Some(path.into())
    } else {
        None
    }
}

fn tls_ca_extra() -> Option<std::ffi::OsString> {
    std::env::var_os("LOGJUICER_CA_EXTRA").or_else(default_ca_extra)
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
    let ca_bundle = tls_ca_bundle();
    let ca_extra = tls_ca_extra();
    if let Some(ca_path) = ca_bundle.as_ref().or(ca_extra.as_ref()) {
        let mut reader = std::io::BufReader::new(std::fs::File::open(ca_path)?);
        let certs = rustls_pemfile::certs(&mut reader)?;
        let mut root_certs = rustls::RootCertStore::empty();
        root_certs.add_parsable_certificates(&certs);

        if ca_extra.is_some() {
            // Add mozilla certificates too, as done by ureq:rtls:root_certs
            root_certs.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
                rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                    ta.subject,
                    ta.spki,
                    ta.name_constraints,
                )
            }));
        }

        let client_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_certs)
            .with_no_client_auth();
        Ok(builder.tls_config(Arc::new(client_config)).build())
    } else {
        Ok(builder.build())
    }
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self::new()
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
