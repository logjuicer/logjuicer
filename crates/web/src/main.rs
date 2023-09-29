// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module is the entrypoint of the logreduce web interface.

use dominator::{clone, html, text, Dom};
use futures_signals::signal::Mutable;
use logreduce_report::{bytes_to_mb, Content, IndexName, LogReport, Report, Source};
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

fn data_attr_html(name: &str, value: &mut [Dom]) -> Dom {
    html!("div", {.class("flex").children(&mut [
        html!("dt", {.class(["w-32", "font-medium", "text-gray-900"]).text(name)}),
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
        html!("td", {.class(["pl-2", "break-all", "whitespace-pre-wrap", color]).text(line)})
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
    let header = html!("div", {.class(["bg-slate-100", "flex", "divide-x", "mr-2"]).children(&mut [
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

    for lr in &report.log_reports {
        childs.push(render_log_report(report, lr))
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

fn render_report_card(report: &Report) -> Dom {
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
        data_attr("Version",    &report.version),
        data_attr("Run time",   &format!("{:.2} sec", report.run_time.as_secs_f32())),
        data_attr("Result",     &result),
    ])})
}

fn render_app(state: &Rc<App>) -> Dom {
    let about = html!("div", {.class(["tooltip", "top-1"]).children(&mut [
        html!("p", {.class("text-gray-700").text("This is logreduce report viewer.")}),
        html!("div", {.class(["hover:bg-slate-400"]).children(&mut [
            render_link("https://github.com/logreduce/logreduce#readme", "documentation")
        ])}),
        data_attr("Viewer", env!("CARGO_PKG_VERSION")),
        data_attr("License", env!("CARGO_PKG_LICENSE")),
    ])});
    html!("div", {.children(&mut [
        html!("nav", {.class(["sticky", "top-0", "bg-slate-300", "z-50", "flex", "px-1", "divide-x"]).children(&mut [
            html!("div", {.class("grow").text("logreduce")}),
            html!("div", {.class(["has-tooltip", "px-2", "flex", "items-center"])
                          .child_signal(state.report.signal_ref(|data| match data {
                              Some(Ok(report)) => Some(html!("div", {.children(&mut [
                                  render_report_card(report),
                                  html!("div", {.class("text-sm").text("info")}),
                              ])})),
                              _ => None
                          }))}),
            html!("div", {.class(["has-tooltip", "px-2", "flex", "items-center"]).children(&mut [
                about,
                html!("div", {.class("text-sm").text("about")}),
            ])})
        ])}),
    ]).child_signal(state.report.signal_ref(|data| Some(match data {
        Some(Ok(report)) => render_report(report),
        Some(Err(err)) => html!("div", {.children(&mut [text("Error: "), text(err)])}),
        None => html!("div", {.text("loading...")}),
    })))})
}

struct App {
    report: Mutable<Option<Result<Report, String>>>,
}

impl App {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            report: Mutable::new(None),
        })
    }
}

async fn get_report(path: &str) -> Result<Report, String> {
    let resp = gloo_net::http::Request::get(path)
        .send()
        .await
        .map_err(|e| format!("{}", e))?;
    let data: Vec<u8> = resp.binary().await.map_err(|e| format!("{}", e))?;
    // log!(format!("Loaded report: {:?}", &data[..24]));
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
    dominator::append_dom(&dominator::body(), render_app(&app));
}
