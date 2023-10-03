// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use warp::Filter;

mod database;
mod routes;

fn with_db(
    workers: database::Workers,
) -> impl Filter<Extract = (database::Workers,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || workers.clone())
}

#[tokio::main]
async fn main() {
    let workers = database::Workers::new();

    let root_api = warp::path::end()
        .and(warp::get())
        .and(with_db(workers.clone()))
        .and_then(routes::reports_list);
    let get_api = warp::path("url")
        .and(warp::path::full())
        .and(warp::get())
        .and(with_db(workers))
        .and_then(routes::report_get);

    let api = root_api.or(get_api);

    println!("Let's go!");
    warp::serve(api).run(([0, 0, 0, 0], 3030)).await;
}
