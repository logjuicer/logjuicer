// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the HTTP logic.

use logreduce_report::report_row::ReportID;
use tracing_subscriber::fmt::format::FmtSpan;
use warp::Filter;

mod database;
mod routes;
mod worker;

fn with_db(
    workers: worker::Workers,
) -> impl Filter<Extract = (worker::Workers,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || workers.clone())
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "tracing=info,warp=info".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let workers = worker::Workers::new().await;

    let list_api = warp::path!("api" / "reports")
        .and(with_db(workers.clone()))
        .and_then(routes::reports_list);

    let get_api = warp::path!("api" / "report" / ReportID)
        .and(warp::get())
        .and_then(routes::report_get);

    let new_api = warp::path!("api" / "report" / "new")
        .and(warp::put())
        .and(with_db(workers.clone()))
        .and(warp::query::<routes::NewReportQuery>())
        .and_then(routes::report_new);

    let watch_api = warp::path!("wsapi" / "report" / ReportID)
        .and(warp::ws())
        .and(with_db(workers))
        .map(|report_id, ws: warp::ws::Ws, workers| {
            ws.on_upgrade(move |socket| routes::report_watch(report_id, socket, workers))
        });

    let any = warp::any().map(routes::index);

    let api = list_api
        .or(get_api)
        .or(new_api)
        .or(watch_api)
        .or(any)
        .with(warp::trace::request());

    warp::serve(api).run(([0, 0, 0, 0], 3030)).await;
}
