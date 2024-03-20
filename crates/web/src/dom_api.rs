// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logjuicer-api web client.

use dominator::{clone, events, html, link, text, with_node, Dom, EventOptions};
use futures_signals::signal::Mutable;
use futures_signals::signal_vec::MutableVec;
use gloo_console::log;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

use logjuicer_report::report_row::{ReportID, ReportKind, ReportRow, ReportStatus};

use crate::dom_utils::*;
use crate::state::{App, Route};

const TH_CLASS: [&str; 2] = ["px-3", "py-2"];

fn render_report_row(state: &Rc<App>, report: &ReportRow) -> Dom {
    let (report_route, name) = match &report.target {
        ReportKind::Similarity => (Route::Similarity(report.id), "similarity"),
        ReportKind::Target(target) => (Route::Report(report.id), target.as_str()),
    };
    let status = match &report.status {
        ReportStatus::Pending => {
            link!(state.to_url(Route::Watch(report.id)), {.text("watch")})
        }
        ReportStatus::Completed => {
            link!(state.to_url(report_route), {.text("read")})
        }
        ReportStatus::Error(err) => {
            link!(state.to_url(report_route), {.text("error").attr("title", &err)})
        }
    };
    html!("tr", {.class(["border-b", "px-6"]).children(&mut [
            html!("td", {.class(TH_CLASS).child(status)}),
            html!("td", {.class(TH_CLASS).text(&format!("{}", report.anomaly_count))}),
            html!("td", {.class(TH_CLASS).text(name)}),
            html!("td", {.class(TH_CLASS).text(&report.baseline)}),
            html!("td", {.class(TH_CLASS).text(&format!("{}", report.updated_at))}),
        ])
    })
}

fn render_report_rows(state: &Rc<App>, reports: &[ReportRow]) -> Dom {
    let mut tbody = reports
        .iter()
        .map(|row| render_report_row(state, row))
        .collect::<Vec<Dom>>();
    html!("table", {.class(["my-6", "w-full", "text-sm", "text-left"]).children(&mut [
        html!("thead", {.class(["bg-slate-100"]).children(&mut [
            html!("th", {.class(TH_CLASS).text("Status")}),
            html!("th", {.class(TH_CLASS).text("Anomaly")}),
            html!("th", {.class(TH_CLASS).text("URL")}),
            html!("th", {.class(TH_CLASS).text("Baseline")}),
            html!("th", {.class(TH_CLASS).text("Updated At")}),
        ])}),
        html!("tbody", {.children(&mut tbody)}),
    ])})
}

fn is_valid_url(url: &str) -> bool {
    url.is_empty()
        || (url.chars().filter(|c| *c == '/').count() > 2 && web_sys::Url::new(url).is_ok())
}

fn render_input(state: &Rc<App>) -> Dom {
    let url = Mutable::new("".to_string());
    let baseline = Mutable::new("".to_string());
    let show_submit = Mutable::new(false);

    html!("form", {.class("grid").class(["shadow-lg", "rounded-md", "py-3", "px-5"]).children(&mut [
        html!("input" => HtmlInputElement, {
            .focused(true)
                .class(["w-full", "rounded", "border", "pl-1"])
                .attr("placeholder", "Target URL")
                .prop_signal("value", url.signal_cloned())

                .with_node!(element => {
                    .event(clone!(show_submit => clone!(url => move |_: events::Input| {
                        let url_value: String = element.value();
                        let _ = if is_valid_url(&url_value) {
                            if !url_value.is_empty() {
                                show_submit.set_neq(true);
                            }
                            element.class_list().remove_1("border-red-500")
                        } else {
                            show_submit.set_neq(false);
                            element.class_list().add_1("border-red-500")
                        };
                        url.set_neq(url_value);
                    })))
                })
        }),
        html!("div", {.class(["flex", "justify-center", "mt-2", "gap-2"]).visible_signal(show_submit.signal()).children(&mut [
            html!("button", {.class(["whitespace-nowrap", "rounded", "px-2", "py-1", "text-white", "font-bold", "bg-blue-500", "hover:bg-blue-700"])
                             .text("LogJuicer Search")}),
            html!("input" => HtmlInputElement, {
                .class(["w-full", "rounded", "border", "pl-1"])
                    .attr("placeholder", "Baseline URL")
                    .prop_signal("value", baseline.signal_cloned())

                    .with_node!(element => {
                        .event(clone!(baseline => move |_: events::Input| {
                            let url_value: String = element.value();
                            let _ = if is_valid_url(&url_value) {
                                element.class_list().remove_1("border-red-500")
                            } else {
                                element.class_list().add_1("border-red-500")
                            };
                            baseline.set_neq(url_value);
                        }))
                    })
            })
        ])}),
    ]).event_with_options(&EventOptions::preventable(), clone!(state => clone!(url => move |ev : events::Submit| {

        let target_url: &str = &(url.lock_mut());
        let baseline_url: &str = &(baseline.lock_mut());
        let baseline = if baseline_url.is_empty() || !is_valid_url(baseline_url) {
            None
        } else {
            Some(baseline_url.into())
        };
        if is_valid_url(target_url) {
            state.visit(Route::NewReport(target_url.into(), baseline));
        }
        ev.prevent_default();
        ev.stop_propagation();
    })))})
}

pub fn do_render_welcome(state: &Rc<App>) -> Dom {
    html!("div", {.class("px-2").children(&mut [
        html!("div", {.class(["font-semibold", "mt-2"]).text("Welcome to the logjuicer web interface!")}),
        render_input(state),
    ])})
}

pub fn do_render_audit(state: &Rc<App>) -> Dom {
    let reports = Mutable::new(None);
    let url = state.reports_url();
    spawn_local(clone!(reports => async move {
        let resp = request_reports(&url).await;
        reports.replace(Some(resp));
    }));
    html!("div", {.class("px-2").children(&mut [
        html!("div", {.class("p-2").child_signal(reports.signal_ref(clone!(state => move |reports| Some(match reports {
            Some(Ok(reports)) => render_report_rows(&state, reports),
            Some(Err(err)) => html!("div", {.children(&mut [text("Error: "), text(err)])}),
            None => html!("div", {.text("loading...")}),
        }))))}),
    ])})
}

async fn request_reports(url: &str) -> Result<Vec<ReportRow>, String> {
    let resp = gloo_net::http::Request::get(url)
        .send()
        .await
        .map_err(|e| format!("request err: {}", e))?;
    if resp.ok() {
        let data = resp.json().await.map_err(|e| format!("json err: {}", e))?;
        Ok(data)
    } else {
        Err(format!(
            "api {} {}",
            resp.status(),
            resp.text().await.unwrap_or("".into()),
        ))
    }
}

pub fn do_render_new(state: &Rc<App>, target: String) -> Dom {
    let result: Mutable<FetchResult<(ReportID, ReportStatus)>> = Mutable::new(None);
    spawn_local(clone!(result => async move {
        let resp = request_new_report(&target).await;
        result.replace(Some(resp));
    }));
    html!("div", {.child_signal(result.signal_ref(clone!(state => move |data| match data {
            Some(Ok((report_id, ReportStatus::Pending))) => {
                Some(do_render_run(&state, *report_id))
            },
            Some(Ok((report_id, ReportStatus::Completed))) => {
                state.replace_url(Route::Report(*report_id));
                None
            },
            Some(Ok((_, ReportStatus::Error(e)))) => Some(html!("div", {.children(&mut [
                text("Processing error: "),
                text(e)
            ])})),
            Some(Err(err)) => Some(html!("div", {.children(&mut [text("Error: "), text(err)])})),
            None => Some(html!("div", {.text("loading...")}))
        })))
    })
}

async fn request_new_report(path: &str) -> Result<(ReportID, ReportStatus), String> {
    let resp = gloo_net::http::Request::put(path)
        .send()
        .await
        .map_err(|e| format!("request err: {}", e))?;
    let data = resp.json().await.map_err(|e| format!("json err: {}", e))?;
    Ok(data)
}

use futures::StreamExt;
use futures_signals::signal_vec::SignalVecExt;
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;
pub fn do_render_run(state: &Rc<App>, report_id: ReportID) -> Dom {
    let infos: MutableVec<Rc<String>> = MutableVec::new();
    let url = state.ws_report_url(report_id);
    let mut ws = WebSocket::open(&url).unwrap();

    let final_id = report_id;
    let handler = clone!(state => clone!(infos => async move {
        while let Some(Ok(Message::Text(msg))) = ws.next().await {
            let done = msg == "Done";
            infos.lock_mut().push_cloned(Rc::new(msg));
            if done {
                gloo_timers::future::TimeoutFuture::new(500).await;
                state.replace_url(Route::Report(final_id));
            }
        }
        log!("WebSocket stream ended!");
        gloo_timers::future::TimeoutFuture::new(1_000).await;
        state.replace_url(Route::Report(final_id));
    }));

    let sig = infos
        .signal_vec_cloned()
        .map(|ev| html!("pre", {.class(["font-mono", "m-2", "ml-4"]).text(&ev)}));

    html!("div", {.future(handler).class("px-2").children_signal_vec(sig)})
}
