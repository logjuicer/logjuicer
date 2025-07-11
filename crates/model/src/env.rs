// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module provides a global environment.

use crate::{
    config::{Config, TargetConfig},
    unordered::KnownLines,
    Source,
};
use anyhow::Result;
use logjuicer_report::Content;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct Env {
    pub client: ureq::Agent,
    pub output: OutputMode,
    pub auth: Option<(Arc<str>, Arc<str>)>,
}

impl Env {
    pub fn new() -> Self {
        Env::new_with_settings(OutputMode::Debug)
    }

    pub fn new_with_settings(output: OutputMode) -> Self {
        let auth = match std::env::var("LOGJUICER_HTTP_AUTH") {
            Ok(value) => match value.split_once(": ") {
                Some((k, v)) => Some((Arc::from(k), Arc::from(v))),
                // TODO: move this check outside of the Env ctor and replace panic with Err() result.
                None => panic!("LOGJUICER_HTTP_AUTH is not valid, it must be of the form 'Header: Value', it was: {}", value)
            },
            Err(_) => None,
        };
        Env {
            client: new_agent(),
            output,
            auth,
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
pub type CurrentTarget = Arc<Mutex<Option<Source>>>;
pub struct TargetEnv<'a> {
    pub config: &'a TargetConfig,
    pub gl: &'a Env,
    pub current: Option<CurrentTarget>,
}

impl TargetEnv<'_> {
    pub fn new_skip_lines(&self) -> Option<KnownLines> {
        self.config.new_skip_lines()
    }
    pub fn set_current(&self, source: &Source) {
        match &self.current {
            None => {}
            Some(current) => {
                let mut data = current.lock().unwrap();
                *data = Some(source.clone())
            }
        }
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
            .map(|p| Config::from_path(Some(&gl), p))
            .unwrap_or_else(|| Ok(Config::default()))?;
        Ok(Self { gl, config })
    }

    pub fn get_target_env_with_current<'a>(
        &'a self,
        target: &Content,
        current: Option<CurrentTarget>,
    ) -> TargetEnv<'a> {
        TargetEnv {
            config: self.config.get_target_config(target),
            gl: &self.gl,
            current,
        }
    }

    pub fn get_target_env<'a>(&'a self, target: &Content) -> TargetEnv<'a> {
        self.get_target_env_with_current(target, None)
    }

    /// Helper function to debug
    pub fn debug_or_progress(&self, msg: &str) {
        self.gl.debug_or_progress(msg)
    }
}

fn new_agent() -> ureq::Agent {
    new_agent_safe().expect("ureq agent creation failed")
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

fn add_certs(
    root_certs: &mut Vec<ureq::tls::Certificate<'static>>,
    fp: std::ffi::OsString,
) -> Result<(), std::io::Error> {
    let data = std::fs::read(fp)?;
    for pem in ureq::tls::parse_pem(&data) {
        if let Ok(ureq::tls::PemItem::Certificate(cert)) = pem {
            root_certs.push(cert)
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Invalid cert".to_string(),
            ));
        }
    }
    Ok(())
}

// Copied from https://github.com/PyO3/maturin/blob/23158969c97418b07a3c4d31282d220ec08c3c10/src/upload.rs#L395-L418
fn new_agent_safe() -> Result<ureq::Agent, std::io::Error> {
    let mut builder = ureq::Agent::config_builder();
    if let Some(proxy) = ureq::Proxy::try_from_env() {
        builder = builder.proxy(Some(proxy));
    };
    let ca_bundle = tls_ca_bundle();
    let ca_extra = tls_ca_extra();
    let config = if std::env::var_os("LOGJUICER_SSL_NO_VERIFY").is_some() {
        let client_config = ureq::tls::TlsConfig::builder().disable_verification(true);
        builder.tls_config(client_config.build()).build()
    } else if ca_bundle.is_some() || ca_extra.is_some() {
        let mut root_certs = Vec::new();

        if let Some(ca_path) = ca_bundle {
            add_certs(&mut root_certs, ca_path)?;
        } else {
            // Add default ca
            for ta in webpki_roots::TLS_SERVER_ROOTS.iter() {
                root_certs.push(ureq::tls::Certificate::from_der(
                    &ta.subject_public_key_info,
                ))
            }
        }
        if let Some(ca_path) = ca_extra {
            add_certs(&mut root_certs, ca_path)?;
        }

        let client_config = ureq::tls::TlsConfig::builder().root_certs(
            ureq::tls::RootCerts::Specific(std::sync::Arc::new(root_certs)),
        );
        builder.tls_config(client_config.build()).build()
    } else {
        builder.build()
    };
    Ok(config.new_agent())
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
