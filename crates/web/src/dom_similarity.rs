// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic to render a similarity report.

use std::{cmp::Ordering, iter::once};

use dominator::{clone, html, Dom};
use futures_signals::signal::Mutable;
use itertools::{chain, Itertools};
use logjuicer_report::{SimilarityAnomalyContext, SimilarityLogReport, SimilarityReport};

use crate::{
    dom_report::{render_anomaly_context, render_content, render_source_link},
    dom_utils::{fetch_data, mk_card, render_link},
    selection::{put_hash_into_view, Selection},
};

fn render_anomaly(
    gl_pos: &mut usize,
    lines: &mut Vec<Dom>,
    slr: &SimilarityLogReport,
    anomaly: &SimilarityAnomalyContext,
) {
    let info = html!("span", {.text("Found in targets: ")});
    let targets = anomaly.sources.iter().map(|sid| {
        let source = &slr.sources[sid.0];
        let target_str = &format!("{}", source.target.0);
        let target_url = source.source.as_str();
        html!("span", {.class("pl-1").children(&mut [
            render_link(target_url, target_str)
        ])})
    });
    lines.push(html!("tr", {.children(&mut [
        html!("td"),
        html!("td", {.children(chain(once(info), targets))})
    ])}));
    render_anomaly_context(gl_pos, &mut None, lines, &anomaly.anomaly)
}

fn render_top(gl_pos: &mut usize, report: &SimilarityReport, count: usize) -> Dom {
    let mut current_src = None;
    let mut lines = Vec::new();
    for (_count, slr, anomaly) in report
        .similarity_reports
        .iter()
        .flat_map(|slr| {
            slr.anomalies
                .iter()
                .map(move |anomaly| (anomaly.sources.len(), slr, anomaly))
        })
        .filter(|(count, _, _)| *count > 1)
        .sorted_by(|a, b| b.0.cmp(&a.0))
        .take(count)
        .sorted_by(|a, b| a.1.sources[0].source.cmp(&b.1.sources[0].source))
    {
        if current_src != Some(&slr.sources[0]) {
            current_src = Some(&slr.sources[0]);
            lines.push(html!("tr", {.children(&mut [
                html!("td"),
                html!("td", {.children(&mut [
                    html!("div", {.class(["mt-5", "flex"]).children(&mut [
                        html!("span", {.class(["pr-1", "font-semibold"]).text("In file: ")}),
                        html!("span", {.text(&slr.sources[0].source.get_relative())}),
                    ])})
                ])})
            ])}));
        }
        render_anomaly(gl_pos, &mut lines, slr, anomaly);
    }
    html!("table", {.children(&mut lines)})
}

fn render_similarity_reports(gl_pos: &mut usize, report: &SimilarityReport) -> Dom {
    let mut childs = Vec::new();
    for slr in &report.similarity_reports {
        childs.push(render_source_link(&slr.sources[0].source));
        let mut lines = Vec::new();
        for anomaly in &slr.anomalies {
            render_anomaly(gl_pos, &mut lines, slr, anomaly);
        }
        childs.push(html!("table", {.children(&mut lines)}));
    }
    html!("div", {.children(childs)})
}

fn render_similarity_matrix(report: &SimilarityReport) -> Dom {
    // initialize the matrix to count shared anomalies
    let mut matrix: Vec<Vec<usize>> = Vec::from_iter(
        report
            .targets
            .iter()
            .map(|_| Vec::from_iter((0..report.targets.len()).map(|_| 0))),
    );

    // initialize the total anomaly count per target
    let mut totals: Vec<usize> = Vec::from_iter((0..report.targets.len()).map(|_| 0));

    for slr in &report.similarity_reports {
        for anomaly in &slr.anomalies {
            // Update the total
            for source in &anomaly.sources {
                let tid = slr.sources[source.0].target.0;
                totals[tid] += 1;
            }
            // Update the matrix
            for pair in anomaly
                .sources
                .iter()
                // do pairwise comparaison
                .permutations(2)
                // only do the lower part of the matrix
                .filter(|pair| pair[0] < pair[1])
            {
                let t1 = slr.sources[pair[0].0].target.0;
                let t2 = slr.sources[pair[1].0].target.0;
                matrix[t1][t2] += 1;
            }
        }
    }

    // compute the relative similarity between two targets
    let get_similarity = |t1: usize, t2: usize| {
        let shared = match t1.cmp(&t2) {
            Ordering::Equal => totals[t1],
            Ordering::Less => matrix[t1][t2],
            Ordering::Greater => matrix[t2][t1],
        };
        let percent = (shared as f32 * 100.0) / (totals[t2] as f32);
        format!("{:.1}%", percent)
    };

    let headers = chain(
        once("Target info".to_string()),
        (0..matrix.len()).map(|col| format!("T-{}", col + 1)),
    )
    .map(|n| html!("th", {.text(&n)}));

    let rows = (0..matrix.len()).map(|row| {
        html!("tr", {.children(chain(
            once(html!("td", {.class(["p-1", "flex", "flex-row", "justify-end"]).children(&mut [
                render_content(&report.targets[row]),
                html!("span", {.class(["font-semibold", "pl-1"]).text(&format!("T-{}", row + 1))}),
            ])})),
            (0..matrix.len()).map(
                |col| html!("td", {.class(["p-1", "text-right"])
                                   .text(&get_similarity(row, col))})
            )))
        })
    });

    html!("table", {.class(["table-auto"]).children(&mut [
        html!("thead", {.children(&mut [html!("tr", {.children(headers)})])}),
        html!("tbody", {.children(rows)}),
    ])})
}

fn render_similarity_report(report: &SimilarityReport) -> Dom {
    let mut gl_pos = 0;
    html!("div", {.children(&mut [
        mk_card("Targets comparaisons", render_similarity_matrix(report)),
        mk_card("Top 100 most common anomalies", render_top(&mut gl_pos, report, 100)),
        mk_card("All anomalies", render_similarity_reports(&mut gl_pos, report)),
    ])})
}

async fn get_report(path: &str) -> Result<SimilarityReport, String> {
    let data = fetch_data(path).await?;
    logjuicer_report::SimilarityReport::load_bytes(&data)
        .map_err(|e| format!("Decode error: {}", e))
}

pub fn fetch_and_render_similarity_report(path: String) -> Dom {
    let report = Mutable::new(None);
    wasm_bindgen_futures::spawn_local(clone!(report => async move {
        // gloo_timers::future::TimeoutFuture::new(3_000).await;
        let result = get_report(&path).await;
        report.replace(Some(result));
        if let Some(selection) = Selection::from_url() {
            put_hash_into_view(selection).await
        }
    }));
    html!("div", {.child_signal(report.signal_ref(|data| Some(match data {
        Some(Ok(report)) => render_similarity_report(report),
        Some(Err(err)) => html!("pre", {.class(["font-mono", "m-2", "ml-4"]).text(err)}),
        None => html!("div", {.text("loading...")}),
    })))})
}
