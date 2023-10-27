// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the HTTP logic.

use axum::routing::{get, put};
use std::str::FromStr;
use tower_http::services::ServeDir;
use tower_http::trace::{self, TraceLayer};

mod database;
mod routes;
mod worker;

fn collect_vstat() {
    if let Ok(statvfs) = rustix::fs::statvfs("data") {
        if let Some(used) = statvfs
            .f_blocks
            .checked_sub(statvfs.f_bavail)
            .map(|f_bused| (f_bused * statvfs.f_bsize) as f64)
        {
            metrics::gauge!("logjuicer_data_used_bytes", used);
        }
        metrics::gauge!(
            "logjuicer_data_available_bytes",
            (statvfs.f_bsize * statvfs.f_bavail) as f64
        );
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("failed to install Prometheus recorder");
    let collector = metrics_process::Collector::default();
    // Call `describe()` method to register help string.
    collector.describe();

    metrics::describe_gauge!(
        "logjuicer_data_used_bytes",
        metrics::Unit::Bytes,
        "Disk usage in bytes."
    );
    metrics::describe_gauge!(
        "logjuicer_data_available_bytes",
        metrics::Unit::Bytes,
        "Disk usage in bytes."
    );

    let workers = worker::Workers::new().await;

    let mut app = axum::Router::new()
        .route("/ready", get(|| async { "ok" }))
        .route("/api/reports", get(routes::reports_list))
        .route("/api/report/:report_id", get(routes::report_get))
        .route("/api/report/new", put(routes::report_new))
        .route("/wsapi/report/:report_id", get(routes::report_watch))
        .route(
            "/metrics",
            get(move || {
                // Collect information just before handle '/metrics'
                collector.collect();
                collect_vstat();
                std::future::ready(handle.render())
            }),
        )
        .with_state(workers)
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(tracing::Level::INFO)),
        );

    if let Ok(assets) = std::env::var("LOGJUICER_ASSETS") {
        let mut base_url = std::env::var("LOGJUICER_BASE_URL").unwrap_or("/".into());
        if !base_url.ends_with('/') {
            base_url.push('/');
        }
        let index =
            axum::response::Html(routes::generate_html(&base_url, env!("CARGO_PKG_VERSION")));
        app = app
            .nest_service("/assets", ServeDir::new(assets).precompressed_gzip())
            .fallback(get(|| std::future::ready(index)))
    }

    let addr = std::net::SocketAddr::from_str("0.0.0.0:3000").unwrap();
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shuting down");
        })
        .await
        .unwrap();
}
