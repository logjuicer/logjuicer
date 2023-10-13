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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let workers = worker::Workers::new().await;

    let mut app = axum::Router::new()
        .route("/api/reports", get(routes::reports_list))
        .route("/api/report/:report_id", get(routes::report_get))
        .route("/api/report/new", put(routes::report_new))
        .route("/wsapi/report/:report_id", get(routes::report_watch))
        .with_state(workers)
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(tracing::Level::INFO)),
        );

    if let Ok(assets) = std::env::var("LOGREDUCE_ASSETS") {
        // TODO: ensure assets is / terminated;
        let index = axum::response::Html(routes::generate_html(
            &std::env::var("LOGREDUCE_BASE_URL").unwrap_or("/".into()),
            env!("CARGO_PKG_VERSION"),
        ));
        app = app
            .nest_service("/assets", ServeDir::new(assets).precompressed_gzip())
            .fallback(get(|| async { index }))
    }

    let addr = std::net::SocketAddr::from_str("0.0.0.0:3000").unwrap();
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
