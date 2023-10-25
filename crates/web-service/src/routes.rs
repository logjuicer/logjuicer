// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the http handler logic.

use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Json;
use futures::TryFutureExt;
use hyper::Body;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use logreduce_report::report_row::{ReportID, ReportRow, ReportStatus};

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

pub async fn report_get(
    State(workers): State<Workers>,
    Path(report_id): Path<ReportID>,
) -> Result<hyper::Response<Body>> {
    let fp = format!("data/{}.gz", report_id);
    if let Ok(file) = File::open(&fp).await {
        // The file exists, stream its content...
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        Ok(hyper::Response::builder()
            .header("Content-Encoding", "gzip")
            .body(body)
            .unwrap())
    } else if let Some(status) = workers
        .db
        .get_report_status(report_id)
        .await
        .map_err(handle_db_error)?
    {
        match status {
            ReportStatus::Pending => Err((
                StatusCode::NOT_FOUND,
                "Report is pending, try again later".into(),
            )),
            ReportStatus::Error(s) => Err((
                StatusCode::NOT_FOUND,
                format!("Report creation failed:\n {s}"),
            )),
            ReportStatus::Completed => {
                Err((StatusCode::NOT_FOUND, "Report is file is missing".into()))
            }
        }
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
    let baseline = args.baseline.as_deref().unwrap_or("auto");
    let report = workers
        .db
        .lookup_report(&args.target, baseline)
        .await
        .map_err(handle_db_error)?;
    match report {
        Some(report) => Ok(Json(report)),
        None => {
            tracing::info!(target = args.target, "Creating a new report");
            let report_id = workers
                .db
                .initialize_report(&args.target, baseline)
                .await
                .map_err(handle_db_error)?;
            workers.submit(report_id, &args.target, args.baseline.as_deref());
            Ok(Json((report_id, ReportStatus::Pending)))
        }
    }
}

pub async fn report_watch(
    ws: WebSocketUpgrade,
    Path(report_id): Path<ReportID>,
    State(workers): State<Workers>,
) -> Result<axum::response::Response> {
    match workers.subscribe(report_id) {
        Some(monitor) => Ok(ws.on_upgrade(move |socket| {
            do_report_watch(monitor, socket)
                .unwrap_or_else(|err| tracing::warn!("websocket handler error: {}", err))
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            "Report is not pending or running".into(),
        )),
    }
}

use axum::extract::ws::{Message, WebSocket};
pub async fn do_report_watch(
    monitor: crate::worker::ProcessMonitor,
    mut ws: WebSocket,
) -> std::result::Result<(), axum::Error> {
    let mut monitor_rx = monitor.chan.subscribe();
    {
        let events = monitor.events.read().await;
        if events.is_empty() {
            ws.send("Waiting to start...".into()).await?;
        } else {
            // Send previous events
            for event in events.iter() {
                ws.send(Message::Text(event.to_string())).await?;
            }
        };
    }

    while let Ok(msg) = monitor_rx.recv().await {
        ws.send(Message::Text(msg.to_string())).await?;
    }
    ws.close().await?;
    Ok(())
}

pub fn generate_html(url_base_path: &str, version: &str) -> String {
    let url = format!("{url_base_path}assets/logreduce-web.");
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>logreduce</title>
<link rel="stylesheet" href="{url}css?v={version}">
<link rel="preload" href="{url}wasm?v={version}" as="fetch" type="application/wasm" crossorigin="">
<link rel="modulepreload" href="{url}js?v={version}">
</head><body><script type="module">import init from '{url}js?v={version}';init('{url}wasm?v={version}');</script></body></html>"#
    )
}
