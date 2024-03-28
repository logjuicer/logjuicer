// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the HTTP logic.

use axum::routing::{get, put};
use axum::{middleware::Next, response::IntoResponse};
use logjuicer_model::config::DiskSizeLimit;
use tower_http::services::ServeDir;
use tower_http::trace::{self, TraceLayer};

mod database;
mod models;
mod routes;
#[cfg(test)]
mod tests;
mod worker;

fn collect_vstat() {
    if let Ok(statvfs) = rustix::fs::statvfs("data") {
        if let Some(used) = statvfs
            .f_blocks
            .checked_sub(statvfs.f_bavail)
            .map(|f_bused| (f_bused * statvfs.f_bsize) as f64)
        {
            metrics::gauge!("logjuicer_data_used_bytes").set(used);
        }
        metrics::gauge!("logjuicer_data_available_bytes")
            .set((statvfs.f_bsize * statvfs.f_bavail) as f64);
    }
}

fn setup_logging() {
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    let filter = match std::env::var_os("LOGJUICER_LOG") {
        None => tracing_subscriber::filter::EnvFilter::from_default_env()
            .add_directive("logjuicer_api=info".parse().unwrap()),
        Some(_) => tracing_subscriber::filter::EnvFilter::from_env("LOGJUICER_LOG"),
    };

    let fmt = tracing_subscriber::fmt::layer()
        .with_target(false)
        .compact();

    tracing_subscriber::registry().with(filter).with(fmt).init();
}

use logjuicer_model::env::{EnvConfig, OutputMode};

fn setup_env() -> anyhow::Result<EnvConfig> {
    match std::env::var_os("LOGJUICER_CONFIG") {
        None => Ok(EnvConfig::new()),
        Some(path) => EnvConfig::new_with_settings(Some(path.into()), OutputMode::Debug),
    }
}

#[tokio::main]
async fn main() {
    setup_logging();

    let env = setup_env().expect("Valid config");

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
    metrics::describe_counter!("http_request", "HTTP request count");
    metrics::describe_counter!("http_request_error", "HTTP request error count");

    let workers = worker::Workers::new(false, "data".into(), DiskSizeLimit::default(), env).await;

    let mut app = axum::Router::new()
        .route("/ready", get(|| async { "ok" }))
        .route("/api/models", get(routes::models_list))
        .route("/api/reports", get(routes::reports_list))
        .route("/api/report/:report_id", get(routes::report_get))
        .route("/api/report/new", put(routes::report_new))
        .route("/api/similarity/new", put(routes::similarity_new))
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
        .layer(axum::middleware::from_fn(track_metrics))
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

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    let local_addr = listener.local_addr().expect("local addr");
    tracing::info!("listening on {}", local_addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shuting down");
        })
        .await
        .unwrap();
}

async fn track_metrics(
    req: http::request::Request<axum::body::Body>,
    next: Next,
) -> impl IntoResponse {
    let is_api = req.uri().path().starts_with("/api");
    let response = next.run(req).await;
    let status = response.status().as_u16();
    if is_api {
        if (200..300).contains(&status) {
            metrics::counter!("http_requests").increment(1);
        } else {
            metrics::counter!("http_requests_error").increment(1);
        }
    }
    response
}
