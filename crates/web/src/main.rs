// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module is the entrypoint of the logreduce web interface.

use dominator::{clone, html, text, Dom};
use futures_signals::signal::Mutable;
use gloo_console::log;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;

// for input event
use dominator::{events, with_node};
use web_sys::{Element, HtmlInputElement};

// for throttle
use futures_signals::signal::SignalExt;

use logreduce_report::{bytes_to_mb, Content, IndexName, LogReport, Report, Source};

fn data_attr_html(name: &str, value: &mut [Dom]) -> Dom {
    html!("div", {.class(["sm:grid", "sm:grid-cols-6", "sm:gap-4", "sm:px-0"]).children(&mut [
        html!("dt", {.class(["text-sm", "font-medium", "text-gray-900"]).text(name)}),
        html!("dd", {.class(["flex", "items-center", "text-sm", "text-gray-700", "sm:col-span-5", "sm:mt-0"]).children(value)})
    ])})
}

fn data_attr(name: &str, value: &str) -> Dom {
    data_attr_html(name, &mut [dominator::text(value)])
}

fn render_link(href: &str, text: &str) -> Dom {
    html!("a", {.class("cursor-pointer").attr("href", href).attr("target", "_black").text(text)})
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

fn render_line(pos: usize, distance: f32, line: &str) -> Dom {
    let sev = (distance * 10.0).round() as usize;
    let color: &str = COLORS.get(sev).unwrap_or(&"c0");

    html!("tr", {.children(&mut [
        html!("td", {.class("pos").text(&format!("{}", pos))}),
        html!("td", {.class(["pl-2", "break-all", color]).text(line)})
    ])})
}

fn log_name(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

fn render_log_report(report: &Report, log_report: &LogReport) -> Dom {
    let index_name = &format!("{}", log_report.index_name);
    let mut infos = Vec::new();
    match report.index_reports.get(&log_report.index_name) {
        Some(index_report) => {
            let mut sources: Vec<Dom> = index_report
                .sources
                .iter()
                .map(|source| html!("div", {.children(&mut [render_source_link(source)])}))
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
    let header = html!("div", {.class(["bg-slate-100", "flex", "divide-x", "mr-2"]).children(&mut [
        html!("div", {.class(["grow", "flex"]).children(&mut [
            render_link(log_report.source.as_str(), log_report.source.get_relative())
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
            lines.push(render_line(prev_pos, 0.0, line));
        }
        lines.push(render_line(
            anomaly.anomaly.pos,
            anomaly.anomaly.distance,
            &anomaly.anomaly.line,
        ));
        for (pos, line) in anomaly.after.iter().enumerate() {
            lines.push(render_line(anomaly.anomaly.pos + 1 + pos, 0.0, line));
        }
    }

    html!("div", {.class(["pl-1", "pt-2", "relative", "max-w-full"]).children(&mut [
        header,
        html!("table", {.class("font-mono").children(&mut [
            html!("thead", {.children(&mut [
                html!("tr", {.children(&mut [html!("th", {.class(["w-12", "min-w-[3rem]"])}), html!("th")])})
            ])}),
            html!("tbody", {.children(&mut lines)})
        ])})
    ])})
}

fn render_error(source: &Source, body: &mut [Dom]) -> Dom {
    html!("div", {.class(["pl-1", "pt-2", "relative", "max-w-full"]).children(&mut [
        html!("div", {.class("bg-red-100").children(&mut [render_source_link(source)])}),
        html!("div", {.children(body)})
    ])})
}

fn render_log_error(source: &Source, error: &str) -> Dom {
    render_error(source, &mut [text("Read failure: "), text(error)])
}

fn render_unknown(source: &Source, index: &IndexName) -> Dom {
    render_error(source, &mut [text("Unknown index: "), text(&index.0)])
}

fn render_report(state: &Arc<App>, report: &Report) -> Dom {
    let result = format!(
        "{:02.2}% reduction (from {} to {})",
        (100.0 - (report.total_anomaly_count as f32 / report.total_line_count as f32) * 100.0),
        report.total_line_count,
        report.total_anomaly_count
    );

    // delay search change by 500ms
    let search_debouncer = state
        .search
        .signal_cloned()
        .throttle(|| gloo_timers::future::TimeoutFuture::new(500))
        .for_each(move |value| {
            let reports = dominator::get_id("reports");
            // reset visibility state
            clear_search(&reports);
            let search_lower = value.to_lowercase();
            if !search_lower.is_empty() {
                toggle_search(&reports, &search_lower);
            }
            async {}
        });
    spawn_local(search_debouncer);

    let card = html!("dl", {.class(["divide-y", "divide-gray-100", "pl-4"]).children(&mut [
        data_attr_html("Target", &mut [render_content(&report.target)]),
        data_attr_html("Baselines", &mut report.baselines.iter().map(render_content).collect::<Vec<Dom>>()),
        data_attr("Created at", &render_time(&report.created_at)),
        data_attr("Run time",   &format!("{:.2} sec", report.run_time.as_secs_f32())),
        data_attr("Result",     &result),
    ])});

    let mut childs = vec![card];

    let mut reports = Vec::with_capacity(report.log_reports.len());
    for lr in LogReport::sorted(&report.log_reports) {
        reports.push(render_log_report(report, lr))
    }
    childs.push(html!("div", {.attr("id", "reports").children(&mut reports)}));
    for (source, err) in &report.read_errors {
        childs.push(render_log_error(source, err));
    }
    for (index, sources) in &report.unknown_files {
        for source in sources {
            childs.push(render_unknown(source, index));
        }
    }

    html!("div", {.children(&mut childs)})
}

fn clear_search(reports: &Element) {
    let hidden = reports.get_elements_by_class_name("hidden");
    for pos in 0..hidden.length() {
        if let Some(elem) = hidden.item(pos) {
            let classes = elem.class_list();
            let _ = classes.remove_1("hidden");
        }
    }
}

fn toggle_search(reports: &Element, search: &str) {
    let reports = reports.children();
    for pos in 0..reports.length() {
        if let Some(report) = reports.item(pos) {
            toggle_search_report(&report, search)
        }
    }
}

fn toggle_search_report(report: &Element, search: &str) {
    let mut found = false;
    let rows = report.get_elements_by_tag_name("tr");
    // skip first row from thead
    for nr in 1..rows.length() {
        if let Some(row) = rows.item(nr) {
            if let Some(content) = row.children().item(1).and_then(|cell| cell.text_content()) {
                if content.to_lowercase().contains(search) {
                    found = true;
                } else {
                    let _ = row.class_list().add_1("hidden");
                }
            }
        }
    }
    if !found {
        let _ = report.class_list().add_1("hidden");
    }
}

fn render_app(state: Arc<App>) -> Dom {
    // Create the DOM nodes
    html!("div", {.children(&mut [
        html!("nav", {.class(["sticky", "top-0", "bg-slate-300", "z-50", "flex", "px-1", "divide-x"]).children(&mut [
            html!("div", {.class("px-2").text("logreduce")}),
            html!("div", {.class("grow").children(&mut [
                html!("input" => HtmlInputElement, {
                    .class(["px-1", "border", "rounded", "w-full", "leading-tight", "focus:outline-none", "focus:shadow-outline", "shadow"])
                    .attr("placeholder", "Search...")
                    // Set the value based on the current search
                    .prop_signal("value", state.search.signal_cloned())
                    // handle input event
                    .with_node!(element => {
                        .event(clone!(state => move |_: events::Input| {
                            // Update the current search
                            state.search.set(element.value());
                        }))
                    })
                }),
            ])}),
            html!("div", {.class(["px-2", "hover:bg-slate-400"]).children(&mut [
                render_link("https://github.com/logreduce/logreduce#readme", "documentation")
            ])}),
            html!("div", {.class(["px-2", "rounder"]).children(&mut [
                html!("span", {.class("font-bold").text("version ")}),
                html!("span", {.text(env!("CARGO_PKG_VERSION"))})
            ])})
        ])}),
    ]).child_signal(state.report.signal_ref(clone!(state => move |data| Some(match data {
        Some(Ok(report)) => render_report(&state, report),
        Some(Err(err)) => html!("div", {.children(&mut [text("Error: "), text(err)])}),
        None => html!("div", {.text("loading...")}),
    }))))})
}

struct App {
    report: Mutable<Option<Result<Report, String>>>,
    search: Mutable<String>,
}

impl App {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            report: Mutable::new(None),
            search: Mutable::new("".into()),
        })
    }
}

async fn get_report(path: &str) -> Result<Report, String> {
    let resp = gloo_net::http::Request::get(path)
        .send()
        .await
        .map_err(|e| format!("{}", e))?;
    let data: Vec<u8> = resp.binary().await.map_err(|e| format!("{}", e))?;
    log!(format!("Loaded report: {} byte", data.len()));
    logreduce_report::Report::load_bytes(&data).map_err(|e| format!("{}", e))
}

pub fn main() {
    console_error_panic_hook::set_once();
    let app = App::new();
    spawn_local(clone!(app => async move {
        // gloo_timers::future::TimeoutFuture::new(3_000).await;
        let result = get_report("report.bin").await;
        app.report.replace(Some(result));
    }));
    dominator::append_dom(&dominator::body(), render_app(app));
}
