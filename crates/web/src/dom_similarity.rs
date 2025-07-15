// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic to render a similarity report.

use std::{cmp::Ordering, collections::HashSet, iter::once, rc::Rc, sync::Arc};

use dominator::{clone, html, text, Dom};
use futures_signals::signal::Mutable;
use itertools::{chain, Itertools};
use logjuicer_report::{
    report_row::ReportID, SimilarityAnomalyContext, SimilarityLogReport, SimilarityReport,
};

use crate::{
    dom_report::{render_anomaly_context, render_content, render_source_link},
    dom_utils::{fetch_data, mk_card, render_link, RenderState},
    selection::{put_hash_into_view, Selection},
    state::{App, Route},
};

fn is_unique(uniques: &mut HashSet<Arc<str>>, anomaly: &SimilarityAnomalyContext) -> bool {
    uniques.insert(anomaly.anomaly.anomaly.line.clone())
}

fn render_anomaly(
    gl_pos: &mut usize,
    lines: &mut Vec<Dom>,
    slr: &SimilarityLogReport,
    anomaly: &SimilarityAnomalyContext,
) {
    let info = html!("span", {.text("Found in targets: ")});
    let targets = anomaly.sources.iter().map(|sid| {
        let source = &slr.sources[sid.0];
        let target_str = &format!("{}", source.target.0 + 1);
        let target_url = source.source.as_str();
        html!("span", {.class("pl-2").children(&mut [
            render_link(target_url, target_str)
        ])})
    });
    lines.push(html!("tr", {.children(&mut [
        html!("td"),
        html!("td", {.children(chain(chain(once(info), targets), once(text("."))))})
    ])}));
    render_anomaly_context(gl_pos, &mut None, lines, &anomaly.anomaly)
}

fn render_top(
    render_state: &mut RenderState,
    report: &SimilarityReport,
    count: usize,
    cmp: fn(usize, usize) -> Ordering,
) -> Dom {
    let mut current_src = None;
    let mut lines = Vec::new();
    for (_count, slr, anomaly) in report
        .similarity_reports
        .iter()
        // Extract the (anomalies source count, slr, anomaly) from every slr into a flat list of event
        .flat_map(|slr| {
            slr.anomalies
                .iter()
                .map(move |anomaly| (anomaly.sources.len(), slr, anomaly))
        })
        // Order by count
        .sorted_by(|a, b| cmp(a.0, b.0))
        // Keep the top most entries
        .take(count)
        .filter(|(_, _, anomaly)| is_unique(&mut render_state.uniques, anomaly))
        // Order by source name
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
        render_anomaly(&mut render_state.gl_pos, &mut lines, slr, anomaly);
    }
    html!("table", {.children(&mut lines)})
}

fn most(a: usize, b: usize) -> Ordering {
    b.cmp(&a)
}

fn least(a: usize, b: usize) -> Ordering {
    b.cmp(&a).reverse()
}

fn render_top_most(render_state: &mut RenderState, report: &SimilarityReport, count: usize) -> Dom {
    render_top(render_state, report, count, most)
}

fn render_top_least(
    render_state: &mut RenderState,
    report: &SimilarityReport,
    count: usize,
) -> Dom {
    render_top(render_state, report, count, least)
}

fn render_similarity_reports(render_state: &mut RenderState, report: &SimilarityReport) -> Dom {
    let mut childs = Vec::new();
    for slr in &report.similarity_reports {
        childs.push(render_source_link(&slr.sources[0].source));
        let mut lines = Vec::new();
        slr.anomalies
            .iter()
            .filter(|anomaly| is_unique(&mut render_state.uniques, anomaly))
            .for_each(|anomaly| render_anomaly(&mut render_state.gl_pos, &mut lines, slr, anomaly));
        if !lines.is_empty() {
            childs.push(html!("table", {.children(&mut lines)}));
        }
    }
    html!("div", {.children(childs)})
}

fn render_similarity_matrix(report: &SimilarityReport, urls: &[String]) -> Dom {
    let infos = report.infos();
    // compute the relative similarity between two targets
    let get_similarity = |t1: usize, t2: usize| {
        let shared = match t1.cmp(&t2) {
            Ordering::Equal => infos.totals[t1],
            Ordering::Less => infos.matrix[t1][t2],
            Ordering::Greater => infos.matrix[t2][t1],
        };
        let percent = (shared as f32 * 100.0) / (infos.totals[t2] as f32);
        format!("{:.1}%", percent)
    };
    let get_similarity_info = |t1: usize, t2: usize| {
        format!(
            "T-{} is {} similar to T-{}",
            t1 + 1,
            get_similarity(t1, t2),
            t2 + 1
        )
    };

    let headers = chain(
        ["Report".to_string(), "Target info".to_string()],
        (0..infos.matrix.len()).map(|col| format!("T-{}", col + 1)),
    )
    .map(|n| html!("th", {.text(&n)}));

    let rows = (0..infos.matrix.len()).map(|row| {
        html!("tr", {.children(chain(
            [html!("td", {.class(["p-1"]).children(&mut [
                render_link(&urls[row], "view")
            ])}),
             html!("td", {.class(["p-1", "flex", "flex-row", "justify-end"]).children(&mut [
                render_content(&report.targets[row]),
                html!("span", {.class(["font-semibold", "pl-1"]).text(&format!("T-{}", row + 1))}),
            ])})],
            (0..infos.matrix.len()).map(
                |col| html!("td", {.class(["p-1", "text-right"])
                                   .attr("title", &get_similarity_info(row, col))
                                   .text(&get_similarity(row, col))})
            )))
        })
    });

    html!("table", {.class(["table-auto"]).children(&mut [
        html!("thead", {.children(&mut [html!("tr", {.children(headers)})])}),
        html!("tbody", {.children(rows)}),
    ])})
}

fn render_similarity_report(state: &Rc<App>, resp: &SimilarityReportResp) -> Dom {
    let mut render_state = RenderState::default();
    let urls: Vec<String> = resp
        .rids
        .iter()
        .map(|rid| state.to_url(Route::Report(*rid)))
        .collect();
    let menu_cls = ["cursor-pointer", "bg-slate-100", "font-semibold", "px-2"];
    let menu = [
        html!("a", {.class(menu_cls).attr("href", "#matrix").text("Summary")}),
        html!("a", {.class(menu_cls).attr("href", "#top100").text("Top 100")}),
        html!("a", {.class(menu_cls).attr("href", "#least100").text("Least 100")}),
        html!("a", {.class(menu_cls).attr("href", "#all").text("All")}),
    ];
    let nav_cls = [
        "sticky",
        "flex",
        "top-[24px]",
        "gap-1",
        "w-full",
        "justify-end",
        "pr-2",
    ];
    html!("div", {.children(&mut [
        html!("nav", {.class(nav_cls).children(menu)}),
        mk_card("Targets comparaisons", "matrix", render_similarity_matrix(&resp.report, &urls)),
        mk_card("Top 100 most common anomalies", "top100", render_top_most(&mut render_state, &resp.report, 100)),
        mk_card("Top 100 least common anomalies", "least100", render_top_least(&mut render_state, &resp.report, 100)),
        mk_card("All anomalies", "all", render_similarity_reports(&mut render_state, &resp.report)),
    ])})
}

struct SimilarityReportResp {
    report: SimilarityReport,
    rids: Vec<ReportID>,
}

async fn get_report(path: &str) -> Result<SimilarityReportResp, String> {
    let resp = fetch_data(path).await?;
    let rids = ReportID::from_sep(&resp.baselines.ok_or("Missing baselines".to_string())?)?;
    let report = logjuicer_report::SimilarityReport::load_bytes(&resp.data)
        .map_err(|e| format!("Decode error: {}", e))?;
    if rids.len() != report.targets.len() {
        Err(format!(
            "Missmatch between baselines and targets: {} != {}",
            rids.len(),
            report.targets.len()
        ))
    } else {
        Ok(SimilarityReportResp { report, rids })
    }
}

pub fn fetch_and_render_similarity_report(state: &Rc<App>, path: String) -> Dom {
    let report = Mutable::new(None);
    wasm_bindgen_futures::spawn_local(clone!(report => async move {
        // gloo_timers::future::TimeoutFuture::new(3_000).await;
        let result = get_report(&path).await;
        report.replace(Some(result));
        if let Some(selection) = Selection::from_url() {
            put_hash_into_view(selection).await
        }
    }));
    html!("div", {.child_signal(report.signal_ref(clone!(state => move |data| Some(match data {
        Some(Ok(report)) => render_similarity_report(&state, report),
        Some(Err(err)) => html!("pre", {.class(["font-mono", "m-2", "ml-4"]).text(err)}),
        None => html!("div", {.text("loading...")}),
    }))))})
}
