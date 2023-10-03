// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use hyper::{Body, Response};
use std::convert::Infallible;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};
use warp::Reply;

use crate::database;
use crate::database::Workers;

pub async fn reports_list(workers: Workers) -> Result<impl warp::Reply, Infallible> {
    let db = workers.database.lock().unwrap();
    let reply = format!("Welcome, I have {} reports for you\n", db.reports.len());
    Ok(warp::reply::html(reply))
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(http::status::StatusCode::NOT_FOUND)
        .body("Not Found".into())
        .unwrap()
}

pub async fn report_get(
    path: warp::path::FullPath,
    workers: Workers,
) -> Result<warp::reply::Response, Infallible> {
    let url = path
        .as_str()
        .trim_start_matches("/url/")
        .trim_end_matches('/');
    let resp = if let Some(url) = url.strip_suffix("/report.bin") {
        // The report.bin has been requested by the viewer.
        let fp = database::report_path(url);
        println!("Serving report for {} with {}", url, fp);
        if let Ok(file) = File::open(&fp).await {
            // The file exists, stream its content...
            let stream = FramedRead::new(file, BytesCodec::new());
            let body = Body::wrap_stream(stream);
            hyper::Response::new(body)
        } else {
            // The report file does not exists.
            println!("Error: unknown report {} for {}", fp, url);
            not_found()
        }
    } else {
        // This is the original report request
        let fp = database::info_path(url);
        if std::path::Path::new(&fp).exists() {
            // The info file exists, we can serve the viewer.
            warp::reply::html("viewer!").into_response()
        } else {
            workers.submit(url);
            warp::reply::html("wait ok!").into_response()
        }
    };
    Ok(resp)
}
