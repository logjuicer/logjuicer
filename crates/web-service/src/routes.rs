// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the http handler logic.

use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Json;

use hyper::Body;
use logreduce_report::report_row::{ReportID, ReportRow, ReportStatus};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::worker::Workers;

type Error = (StatusCode, String);
type Result<T> = std::result::Result<T, Error>;

fn handle_db_error(err: sqlx::Error) -> Error {
    tracing::error!("DB error: {}", err);
    (
        StatusCode::SERVICE_UNAVAILABLE,
        format!("Database error {}", err),
    )
}

pub async fn reports_list(State(workers): State<Workers>) -> Result<Json<Vec<ReportRow>>> {
    let reports = workers.db.get_reports().await.map_err(handle_db_error)?;
    Ok(Json(reports))
}

pub async fn report_get(Path(report_id): Path<ReportID>) -> Result<hyper::Response<Body>> {
    let fp = format!("data/{}.bin", report_id);
    if let Ok(file) = File::open(&fp).await {
        // The file exists, stream its content...
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        Ok(hyper::Response::new(body))
    } else {
        Err((StatusCode::NOT_FOUND, "Report Not Found".into()))
    }
}

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
pub struct NewReportQuery {
    target: String,
    baseline: Option<String>,
}

pub async fn report_new(
    State(workers): State<Workers>,
    Query(args): Query<NewReportQuery>,
) -> Result<Json<(ReportID, ReportStatus)>> {
    // TODO: support custom baseline
    if args.baseline.is_some() {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            "baseline is not supported".into(),
        ));
    };
    let report = workers
        .db
        .lookup_report(&args.target)
        .await
        .map_err(handle_db_error)?;
    match report {
        Some(report) => Ok(Json(report)),
        None => {
            tracing::info!(target = args.target, "Creating a new report");
            let report_id = workers
                .db
                .initialize_report(&args.target, args.baseline.as_deref().unwrap_or("auto"))
                .await
                .map_err(handle_db_error)?;
            workers.submit(report_id, &args.target);
            Ok(Json((report_id, ReportStatus::Pending)))
        }
    }
}

pub async fn report_watch(
    ws: WebSocketUpgrade,
    Path(report_id): Path<ReportID>,
    State(workers): State<Workers>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| do_report_watch(report_id, socket, workers))
}

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt, TryFutureExt};
pub async fn do_report_watch(report_id: ReportID, ws: WebSocket, workers: Workers) {
    // Split the socket into a sender and receive of messages.
    let (mut user_ws_tx, mut _user_ws_rx) = ws.split();

    let monitor = workers.subscribe(report_id).unwrap();
    let mut monitor_rx = monitor.chan.subscribe();
    {
        let events = monitor.events.read().await;
        if events.is_empty() {
            user_ws_tx
                .send("Waiting to start...".into())
                .unwrap_or_else(|e| {
                    eprintln!("websocket send error: {}", e);
                    panic!("stop?");
                })
                .await;
        } else {
            // Send previous events
            for event in events.iter() {
                user_ws_tx
                    .send(Message::Text(event.to_string()))
                    .unwrap_or_else(|e| {
                        eprintln!("websocket send error: {}", e);
                        panic!("stop?");
                    })
                    .await;
            }
        };
    }

    while let Ok(msg) = monitor_rx.recv().await {
        user_ws_tx
            .send(Message::Text(msg.to_string()))
            .unwrap_or_else(|e| {
                eprintln!("websocket send error: {}", e);
                panic!("stop?");
            })
            .await;
    }
}

pub async fn index() -> &'static str {
    "NOT IMPLEMENTED!"
}
