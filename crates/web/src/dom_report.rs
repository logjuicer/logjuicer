// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic to render a single report.

use dominator::{clone, html, text, Dom};
use gloo_console::log;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

use logreduce_report::{bytes_to_mb, Content, IndexName, LogReport, Report, Source};

use crate::dom_utils::{data_attr, data_attr_html, render_link};
use crate::selection::Selection;

#[cfg(feature = "api_client")]
use crate::state::App;

#[cfg(not(feature = "api_client"))]
use futures_signals::signal::Mutable;
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

fn render_source_link(source: &Source) -> Dom {
    render_link(source.as_str(), log_name(source.get_relative()))
}

fn render_time(system_time: &std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::offset::Utc> = (*system_time).into();
    datetime.format("%Y-%m-%d %T").to_string()
}

fn render_content(content: &Content) -> Dom {
    match content {
        Content::Zuul(zuul_build) => html!("div", {.children(&mut [
            render_link(&zuul_build.build_url(),
                        &format!("zuul<job={}, project={}, branch={}, result={}>", zuul_build.job_name, zuul_build.project, zuul_build.branch, zuul_build.result))
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

fn log_name(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

fn render_log_report(gl_pos: &mut usize, report: &Report, log_report: &LogReport) -> Dom {
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

    let info_btn = html!("div", {.class(["has-tooltip", "px-2"]).children(&mut [
        html!("div", {.class("tooltip").children(&mut infos)}),
        html!("div", {.class(["font-bold", "text-slate-500"]).text("?")})
    ])});
    let header = html!("header", {.class(["header", "bg-slate-100", "flex", "divide-x", "mr-2"]).children(&mut [
        html!("div", {.class(["grow", "flex"]).children(&mut [
            render_link(log_report.source.get_href(&report.target), log_report.source.get_relative())
        ])}),
        info_btn
    ])});

    let mut lines = Vec::with_capacity(log_report.anomalies.len() * 2);
    for anomaly in &log_report.anomalies {
        for (pos, line) in anomaly.before.iter().enumerate() {
            let prev_pos = anomaly
                .anomaly
                .pos
                .saturating_sub(anomaly.before.len() - pos);
            lines.push(render_line(gl_pos, prev_pos, 0.0, line));
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
    }

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

fn render_report(report: &Report) -> Dom {
    let mut childs = Vec::new();

    let mut gl_pos = 0;
    for lr in &report.log_reports {
        childs.push(render_log_report(&mut gl_pos, report, lr))
    }
    for (source, err) in &report.read_errors {
        childs.push(render_log_error(&report.target, source, err));
    }
    for (index, sources) in &report.unknown_files {
        for source in sources {
            childs.push(render_unknown(&report.target, source, index));
        }
    }

    html!("div", {.children(&mut childs)})
}

pub fn render_report_card(report: &Report) -> Dom {
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
    ])})
}

use gloo_timers::future::TimeoutFuture;
// This function waits for the hash to be rendered
async fn put_hash_into_view(selection: Selection) {
    let body = web_sys::window().unwrap().document().unwrap();
    let elem_id = selection.elem_id();
    for _retry in 0..10 {
        if let Some(elem) = body.get_element_by_id(&elem_id) {
            log!(&format!("Putting {} into view", elem_id));
            elem.scroll_into_view_with_scroll_into_view_options(
                web_sys::ScrollIntoViewOptions::new()
                    .behavior(web_sys::ScrollBehavior::Smooth)
                    .block(web_sys::ScrollLogicalPosition::Center)
                    .inline(web_sys::ScrollLogicalPosition::Center),
            );
            selection.highlight();
            break;
        }
        log!(&format!("Waiting for {}", elem_id));
        TimeoutFuture::new(200).await;
    }
}

async fn get_report(path: &str) -> Result<Report, String> {
    let resp = gloo_net::http::Request::get(path)
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;
    if !resp.ok() {
        return Err(format!("Bad status: {}", resp.status()));
    }
    let data: Vec<u8> = resp
        .binary()
        .await
        .map_err(|e| format!("Response error: {}", e))?;
    // log!(format!("Loaded report: {:?}", &data[..24]));
    logreduce_report::Report::load_bytes(&data).map_err(|e| format!("Decode error: {}", e))
}

pub fn fetch_and_render_report(state: &Rc<App>, path: String) -> Dom {
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
        Some(Err(err)) => html!("div", {.children(&mut [text("Error: "), text(err)])}),
        None => html!("div", {.text("loading...")}),
    })))})
}
