// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the http handler logic.

use hyper::{Body, Response};
use logreduce_report::report_row::{ReportID, ReportStatus};
use std::convert::Infallible;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::worker::Workers;

pub async fn reports_list(workers: Workers) -> Result<impl warp::Reply, Infallible> {
    let reports = workers.db.get_reports().await.unwrap();
    Ok(warp::reply::json(&reports))
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(http::status::StatusCode::NOT_FOUND)
        .body("Not Found".into())
        .unwrap()
}

pub async fn report_get(report_id: ReportID) -> Result<warp::reply::Response, Infallible> {
    let fp = format!("data/{}.bin", report_id);
    let resp = if let Ok(file) = File::open(&fp).await {
        // The file exists, stream its content...
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        hyper::Response::new(body)
    } else {
        // The report file does not exists.
        not_found()
    };
    Ok(resp)
}

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
pub struct NewReportQuery {
    target: String,
    baseline: Option<String>,
}

use warp::Reply;
pub async fn report_new(
    workers: Workers,
    args: NewReportQuery,
) -> Result<warp::reply::Response, Infallible> {
    // TODO: support custom baseline
    if args.baseline.is_some() {
        panic!("baseline is not supported")
    };
    let report = workers.db.lookup_report(&args.target).await.unwrap();
    let reply = match report {
        Some(report) => warp::reply::json(&report),
        None => {
            let report_id = workers
                .db
                .initialize_report(&args.target, args.baseline.as_deref().unwrap_or("auto"))
                .await
                .unwrap();
            workers.submit(report_id, &args.target);
            warp::reply::json(&(report_id, ReportStatus::Pending))
        }
    };
    Ok(reply.into_response())
}

use futures::{SinkExt, StreamExt, TryFutureExt};
use warp::filters::ws::Message;
use warp::ws::WebSocket;
pub async fn report_watch(report_id: ReportID, ws: WebSocket, workers: Workers) {
    // Split the socket into a sender and receive of messages.
    let (mut user_ws_tx, mut _user_ws_rx) = ws.split();

    let monitor = workers.subscribe(report_id).unwrap();
    let mut monitor_rx = monitor.chan.subscribe();
    {
        let events = monitor.events.read().await;
        if events.is_empty() {
            user_ws_tx
                .send(Message::text("Waiting to start..."))
                .unwrap_or_else(|e| {
                    eprintln!("websocket send error: {}", e);
                    panic!("stop?");
                })
                .await;
        } else {
            // Send previous events
            for event in events.iter() {
                user_ws_tx
                    .send(Message::text(&**event))
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
            .send(Message::text(&*msg))
            .unwrap_or_else(|e| {
                eprintln!("websocket send error: {}", e);
                panic!("stop?");
            })
            .await;
    }
}

pub fn index() -> &'static str {
    "NOT IMPLEMENTED!"
}
