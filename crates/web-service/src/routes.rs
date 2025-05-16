// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the http handler logic.

use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Json;
use futures::TryFutureExt;
use logjuicer_report::model_row::ModelRow;
use tokio::fs::File;

use logjuicer_report::report_row::{ReportID, ReportRow, ReportStatus};

use crate::database::report_path;
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

pub async fn models_list(State(workers): State<Workers>) -> Result<Json<Vec<ModelRow>>> {
    let models = workers.db.get_models().await.map_err(handle_db_error)?;
    Ok(Json(models))
}

async fn do_report_get(
    json: bool,
    State(workers): State<Workers>,
    Path(report_id): Path<ReportID>,
) -> Result<axum::response::Response> {
    if let Some((status, baseline)) = workers
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
                let fp = report_path(&workers.storage_dir, report_id);
                if json {
                    report_convert_json(std::path::Path::new(&fp), &baseline)
                } else if let Ok(file) = File::open(&fp).await {
                    // The file exists, stream its content...

                    // Wrap to a tokio_util::io::ReaderStream
                    let reader_stream = tokio_util::io::ReaderStream::new(file);

                    Ok(axum::response::Response::builder()
                        .header("Content-Encoding", "gzip")
                        .header("x-baselines", &baseline)
                        .body(axum::body::Body::from_stream(reader_stream))
                        .unwrap())
                } else {
                    Err((StatusCode::NOT_FOUND, "Report is file is missing".into()))
                }
            }
        }
    } else {
        Err((StatusCode::NOT_FOUND, "Report Not Found".into()))
    }
}

pub fn report_convert_json(
    fp: &std::path::Path,
    baseline: &str,
) -> Result<axum::response::Response> {
    match logjuicer_report::Report::load(fp) {
        Ok(report) => match serde_json::to_vec(&report) {
            Ok(buf) => Ok(axum::response::Response::builder()
                .header("Content-Type", "application/json")
                .header("x-baselines", baseline)
                .body(buf.into())
                .unwrap()),
            Err(error) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Encode failed: {}", error),
            )),
        },
        Err(error) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Load failed: {}", error),
        )),
    }
}

pub async fn report_get(
    workers: State<Workers>,
    report_id: Path<ReportID>,
) -> Result<axum::response::Response> {
    do_report_get(false, workers, report_id).await
}

pub async fn report_get_json(
    workers: State<Workers>,
    report_id: Path<ReportID>,
) -> Result<axum::response::Response> {
    do_report_get(true, workers, report_id).await
}

pub async fn report_status(
    State(workers): State<Workers>,
    Path(report_id): Path<ReportID>,
) -> Result<Json<ReportStatus>> {
    if let Some((status, _)) = workers
        .db
        .get_report_status(report_id)
        .await
        .map_err(handle_db_error)?
    {
        Ok(Json(status))
    } else {
        Err((StatusCode::NOT_FOUND, "Report Not Found".into()))
    }
}

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
pub struct NewReportQuery {
    pub target: String,
    pub baseline: Option<String>,
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
            workers.submit(report_id, ReportRequest::NewReport(args));
            Ok(Json((report_id, ReportStatus::Pending)))
        }
    }
}

pub enum ReportRequest {
    NewReport(NewReportQuery),
    NewSimilarity(Vec<ReportID>),
}

impl ReportRequest {
    #[cfg(test)]
    pub fn new_request(target: String, baseline: String) -> ReportRequest {
        ReportRequest::NewReport(NewReportQuery {
            target,
            baseline: Some(baseline),
        })
    }
}

impl std::fmt::Display for ReportRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportRequest::NewReport(args) => {
                write!(
                    f,
                    "target={}, baseline={}",
                    args.target,
                    args.baseline.as_deref().unwrap_or("auto")
                )
            }
            ReportRequest::NewSimilarity(rids) => {
                write!(f, "similarity={:?}", rids)
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct NewSimilarityQuery {
    reports: String,
}

pub async fn similarity_new(
    State(workers): State<Workers>,
    Query(args): Query<NewSimilarityQuery>,
) -> Result<Json<(ReportID, ReportStatus)>> {
    let report = workers
        .db
        .lookup_report("similarity", &args.reports)
        .await
        .map_err(handle_db_error)?;
    match report {
        Some(report) => Ok(Json(report)),
        None => {
            tracing::info!(reports = args.reports, "Creating a new similarity report");
            let rids = ReportID::from_sep(&args.reports)
                .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;
            let report_id = workers
                .db
                .initialize_report("similarity", &args.reports)
                .await
                .map_err(handle_db_error)?;
            workers.submit(report_id, ReportRequest::NewSimilarity(rids));
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
    mut monitor: crate::worker::ProcessFollower,
    mut ws: WebSocket,
) -> std::result::Result<(), axum::Error> {
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

    let timeout_duration = tokio::time::Duration::from_millis(5_000);
    loop {
        match tokio::time::timeout(timeout_duration, monitor.chan.recv()).await {
            Err(_) => ws.send(Message::Text("...".to_string())).await?,
            Ok(Ok(msg)) => ws.send(Message::Text(msg.to_string())).await?,
            Ok(Err(_)) => break,
        }
    }
    ws.close().await?;
    Ok(())
}

pub fn generate_html(url_base_path: &str, version: &str) -> String {
    let url = format!("{url_base_path}assets/logjuicer-web.");
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>LogJuicer</title>
<link rel="icon" href="{url_base_path}assets/LogJuicer.svg" />
<link rel="stylesheet" href="{url}css?v={version}">
<link rel="preload" href="{url}wasm?v={version}" as="fetch" type="application/wasm" crossorigin="">
<link rel="modulepreload" href="{url}js?v={version}">
</head><body><script type="module">import init from '{url}js?v={version}';init('{url}wasm?v={version}');</script></body></html>"#
    )
}
