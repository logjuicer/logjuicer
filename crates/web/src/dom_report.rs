// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic to render a single report.

use dominator::{clone, html, text, Dom};
use futures_signals::signal::Mutable;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

use logjuicer_report::{
    bytes_to_mb, AnomalyContext, Content, Epoch, IndexName, LogReport, Report, Source,
};

use crate::dom_utils::{data_attr, data_attr_html, fetch_data, render_link};
use crate::selection::{put_hash_into_view, Selection};

#[cfg(feature = "api_client")]
use crate::state::App;

#[cfg(not(feature = "api_client"))]
pub struct App {
    pub report: Mutable<Option<Result<Report, String>>>,
}
#[cfg(not(feature = "api_client"))]
impl App {
    pub fn new() -> Self {
        Self {
            report: Mutable::new(None),
        }
    }
}

pub fn render_source_link(source: &Source) -> Dom {
    render_link(source.as_str(), log_name(source.get_relative()))
}

fn render_time(system_time: &std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::offset::Utc> = (*system_time).into();
    datetime.format("%Y-%m-%d %T").to_string()
}

pub fn render_content(content: &Content) -> Dom {
    match content {
        Content::Zuul(zuul_build) => html!("div", {.children(&mut [
            render_link(&zuul_build.build_url(),
                        &format!("zuul<change={} date={} job={}, project={}, branch={}, result={}>",
                                 zuul_build.change, zuul_build.end_time, zuul_build.job_name, zuul_build.project, zuul_build.branch, zuul_build.result))
        ])}),
        _ => html!("div", {.text(&content.to_string())}),
    }
}

static COLORS: &[&str] = &["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9"];

use wasm_bindgen::JsCast;
fn click_handler(ev: dominator::events::Click) {
    if let Some(target) = ev.target() {
        // Get the line number element.
        let elem = target.dyn_ref::<web_sys::Element>().unwrap();
        // Get the global id.
        let elem_pos = Selection::parse_id(&elem.id()).unwrap();

        if ev.shift_key() {
            Selection::update(elem_pos)
        } else {
            Selection::set(elem_pos)
        };
    }
}

fn render_line(gl_pos: &mut usize, pos: usize, distance: f32, line: &str) -> Dom {
    let sev = (distance * 10.0).round() as usize;
    let color: &str = COLORS.get(sev).unwrap_or(&"c0");
    let pos_str = format!("{}", pos);

    // Create global id.
    let gl_str = Selection::mk_id(*gl_pos);
    *gl_pos += 1;

    html!("tr", {.children(&mut [
        html!("td", {.class("pos").attr("id", &gl_str).text(&pos_str).event(click_handler)}),
        html!("td", {.class(["pl-2", "break-all", "whitespace-pre-wrap", color]).text(line)})
    ])})
}

fn render_sep() -> Dom {
    html!("tr", {.children(&mut [html!("td", {.class(["bg-slate-100", "h-3"]).attr("colspan", "2")})])})
}

pub fn render_anomaly_context(
    gl_pos: &mut usize,
    last_pos: &mut Option<usize>,
    lines: &mut Vec<Dom>,
    anomaly: &AnomalyContext,
) {
    let last = last_pos.unwrap_or(usize::MAX);
    for (pos, line) in anomaly.before.iter().enumerate() {
        let prev_pos = anomaly
            .anomaly
            .pos
            .saturating_sub(anomaly.before.len() - pos);

        if pos == 0 && last < prev_pos {
            // Add separator
            lines.push(render_sep());
        }

        lines.push(render_line(gl_pos, prev_pos, 0.0, line));
    }

    if anomaly.before.is_empty() && last < anomaly.anomaly.pos {
        // Add seperator when there was no before context.
        lines.push(render_sep());
    }

    lines.push(render_line(
        gl_pos,
        anomaly.anomaly.pos,
        anomaly.anomaly.distance,
        &anomaly.anomaly.line,
    ));
    for (pos, line) in anomaly.after.iter().enumerate() {
        let after_pos = anomaly.anomaly.pos + 1 + pos;
        lines.push(render_line(gl_pos, after_pos, 0.0, line));
    }
    *last_pos = Some(anomaly.anomaly.pos + anomaly.after.len() + 1);
}

fn log_name(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

fn render_log_report(
    gl_pos: &mut usize,
    anchor: &str,
    report: &Report,
    log_report: &LogReport,
) -> Dom {
    let index_name = &format!("{}", log_report.index_name);
    let mut infos = Vec::new();
    match report.index_reports.get(&log_report.index_name) {
        Some(index_report) => {
            let mut sources: Vec<Dom> = index_report
                .sources
                .iter()
                .map(|source| html!("div", {.class("pr-2").children(&mut [render_source_link(source)])}))
                .collect();
            infos.push(data_attr_html("Baselines", &mut sources));
            infos.push(data_attr("Index", index_name));
            infos.push(data_attr(
                "Training time",
                &format!("{} ms", index_report.train_time.as_millis()),
            ));
        }
        None => infos.push(data_attr("Unknown Index", index_name)),
    };
    infos.push(data_attr(
        "Test time",
        &format!("{} ms", log_report.test_time.as_millis()),
    ));
    infos.push(data_attr(
        "Anomaly count",
        &format!("{}", log_report.anomalies.len()),
    ));
    infos.push(data_attr(
        "Log size",
        &format!(
            "{} lines, {:.3} MB",
            log_report.line_count,
            bytes_to_mb(log_report.byte_count)
        ),
    ));

    let toggle_info = Mutable::new(false);
    let handler = clone!(toggle_info => move |_: dominator::events::Click| {
        toggle_info.set(!toggle_info.get());
    });
    let info_btn = html!("div", {.class(["has-tooltip", "px-2"]).event(handler).children(&mut [
        html!("div", {.class("tooltip")
                      .class_signal("tooltip-visible", toggle_info.signal()).children(&mut infos)}),
        html!("div", {.class(["font-bold", "text-slate-500"]).text("?")
                      .class_signal("font-extrabold", toggle_info.signal())})
    ])});
    let header = html!("header", {.class(["header", "bg-slate-100", "flex", "divide-x", "mr-2"]).children(&mut [
        html!("div", {.class(["grow", "flex"]).attr("id", anchor).children(&mut [
            render_link(log_report.source.get_href(&report.target), log_report.source.get_relative())
        ])}),
        info_btn
    ])});

    let mut lines = Vec::with_capacity(log_report.anomalies.len() * 2);
    let mut last_pos = None;
    log_report
        .anomalies
        .iter()
        .for_each(|anomaly| render_anomaly_context(gl_pos, &mut last_pos, &mut lines, anomaly));

    html!("div", {.class(["content", "pl-1", "pt-2", "relative", "max-w-full"]).children(&mut [
        header,
        html!("table", {.class("font-mono").children(&mut [
            html!("thead", {.children(&mut [
                html!("tr", {.children(&mut [html!("th", {.class(["w-12", "min-w-[3rem]"])}), html!("th")])})
            ])}),
            html!("tbody", {.children(&mut lines)})
        ])})
    ])})
}

fn render_error(target: &Content, source: &Source, body: &mut [Dom]) -> Dom {
    html!("div", {.class(["pl-1", "pt-2", "relative", "max-w-full"]).children(&mut [
        html!("div", {.class("bg-red-100").children(&mut [render_link(source.get_href(target), source.get_relative())])}),
        html!("div", {.children(body)})
    ])})
}

fn render_log_error(target: &Content, source: &Source, error: &str) -> Dom {
    render_error(target, source, &mut [text("Read failure: "), text(error)])
}

fn render_unknown(target: &Content, source: &Source, index: &IndexName) -> Dom {
    render_error(
        target,
        source,
        &mut [text("Unknown index: "), text(index.as_str())],
    )
}

fn render_timeline(
    gl_pos: &mut usize,
    anchors: &ReportAnchors<'_>,
    timeline: BTreeMap<Epoch, (&LogReport, &AnomalyContext)>,
) -> Dom {
    let mut lines = Vec::with_capacity(timeline.len() * 2);
    let mut current_source = None;
    let mut last_pos = None;
    for (lr, anomaly) in timeline.into_values() {
        if Some(&lr.source) != current_source {
            current_source = Some(&lr.source);
            let link = anchors.get(&lr.source).expect("known source anchor");
            last_pos = None;
            lines.push(html!("tr", {.children(&mut [
                html!("td", {.class(["header2", "text-end", "bg-slate-50", "px-2", "pb-1"])
                             .attr("colspan", "2")
                             .children(&mut [
                                 html!("a", {.class("cursor-pointer").attr("href", &link.1)
                                             .text(lr.source.get_relative())})
                             ])})
            ])}))
        }
        render_anomaly_context(gl_pos, &mut last_pos, &mut lines, anomaly);
    }
    let header = html!("header", {.class(["header", "bg-slate-100", "flex", "divide-x", "mr-2"]).children(&mut [
        html!("div", {.class(["grow", "flex"]).text(
            "Timeline"
        )}),
    ])});

    html!("div", {.class(["content", "pl-1", "pt-2", "relative", "max-w-full"]).children(&mut [
        header,
        html!("table", {.class("font-mono").children(&mut [
            html!("thead", {.children(&mut [
                html!("tr", {.children(&mut [html!("th", {.class(["w-12", "min-w-[3rem]"])}), html!("th")])})
            ])}),
            html!("tbody", {.children(&mut lines)})
        ])})
    ])})
}

type ReportAnchors<'a> = HashMap<&'a Source, (String, String)>;

fn render_report<'a>(report: &'a Report) -> Dom {
    let mut childs = Vec::new();
    let mut gl_pos = 0;

    let anchors: ReportAnchors<'a> =
        HashMap::from_iter(report.log_reports.iter().enumerate().map(|lr| {
            (
                &lr.1.source,
                (format!("lr-{}", lr.0), format!("#lr-{}", lr.0)),
            )
        }));

    if report.log_reports.len() > 1 {
        let mut timeline = BTreeMap::new();
        let mut lr_count = 0;
        for lr in &report.log_reports {
            let mut first = true;
            for (mut ts, anomaly) in lr.timed() {
                // Increase timeline resolution
                ts.0 *= 1_000_000;

                // Count the number of LogReport with timestamp
                if first {
                    lr_count += 1;
                    first = false;
                }

                // Find a free slot
                let mut attempts = 1;
                while timeline.contains_key(&ts) && attempts < 4096 && ts.0 < u64::MAX {
                    attempts += 1;
                    ts.0 += 1;
                }

                timeline.insert(ts, (lr, anomaly));
            }
        }
        if lr_count > 1 {
            childs.push(render_timeline(&mut gl_pos, &anchors, timeline));
        }
    }

    for lr in &report.log_reports {
        let anchor = anchors.get(&lr.source).expect("Known source anchor");
        childs.push(render_log_report(&mut gl_pos, &anchor.0, report, lr))
    }

    if !report.read_errors.is_empty() || !report.unknown_files.is_empty() {
        let toggle_info = Mutable::new(false);
        let handler = clone!(toggle_info => move |_: dominator::events::Click| {
            toggle_info.set(!toggle_info.get());
        });
        childs.push(html!("div", {.class(["pl-1", "pt-2", "bg-red-50", "max-w-full", "cursor-pointer"])
                                  .event(handler)
                                  .text("× Click to show the files that were not processed. They were likely not found in the baseline. ×")}));
        let mut errors = vec![];
        for (source, err) in &report.read_errors {
            errors.push(render_log_error(&report.target, source, err));
        }
        for (index, sources) in &report.unknown_files {
            for source in sources {
                errors.push(render_unknown(&report.target, source, index));
            }
        }
        childs.push(html!("div", {.visible_signal(toggle_info.signal()).children(&mut errors)}));
    }
    html!("div", {.children(&mut childs)})
}

pub fn render_report_card(report: &Report, toggle_info: &Mutable<bool>) -> Dom {
    let result = format!(
        "{:02.2}% reduction (from {} to {})",
        (100.0 - (report.total_anomaly_count as f32 / report.total_line_count as f32) * 100.0),
        report.total_line_count,
        report.total_anomaly_count
    );

    html!("dl", {.class(["tooltip", "top-1", "divide-y", "divide-gray-100", "pl-4"]).children(&mut [
        data_attr_html("Target", &mut [render_content(&report.target)]),
        data_attr_html("Baselines", &mut report.baselines.iter().map(render_content).collect::<Vec<Dom>>()),
        data_attr("Created at", &render_time(&report.created_at)),
        data_attr("Run time",   &format!("{:.2} sec", report.run_time.as_secs_f32())),
        data_attr("Result",     &result),
    ]).class_signal("tooltip-visible", toggle_info.signal())})
}

async fn get_report(path: &str) -> Result<Report, String> {
    let data = fetch_data(path).await?;
    logjuicer_report::Report::load_bytes(&data).map_err(|e| format!("Decode error: {}", e))
}

pub fn fetch_and_render_report(state: &Rc<App>, path: String) -> Dom {
    state.report.set_neq(None);
    spawn_local(clone!(state => async move {
        // gloo_timers::future::TimeoutFuture::new(3_000).await;
        let result = get_report(&path).await;
        state.report.replace(Some(result));
        if let Some(selection) = Selection::from_url() {
            put_hash_into_view(selection).await
        }
    }));
    html!("div", {.child_signal(state.report.signal_ref(|data| Some(match data {
        Some(Ok(report)) => render_report(report),
        Some(Err(err)) => html!("pre", {.class(["font-mono", "m-2", "ml-4"]).text(err)}),
        None => html!("div", {.text("loading...")}),
    })))})
}
