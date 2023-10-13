// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the HTTP logic.

use axum::routing::{get, put};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod database;
mod routes;
mod worker;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "logreduce_web=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let workers = worker::Workers::new().await;

    let app = axum::Router::new()
        .route("/api/reports", get(routes::reports_list))
        .route("/api/report/:report_id", get(routes::report_get))
        .route("/api/report/new", put(routes::report_new))
        .route("/wsapi/report/:report_id", get(routes::report_watch))
        .route("/*path", get(routes::index))
        .with_state(workers);

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
