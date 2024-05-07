// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logjuicer-api web client.

use dominator::{clone, events, html, link, text, with_node, Dom, EventOptions};
use futures_signals::signal::Mutable;
use futures_signals::signal_vec::MutableVec;
use gloo_console::log;
use itertools::Itertools;
use std::collections::HashSet;
use std::ops::Deref;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

use logjuicer_report::report_row::{ReportID, ReportKind, ReportRow, ReportStatus};

use crate::dom_utils::*;
use crate::state::{App, Route};

const TH_CLASS: [&str; 2] = ["px-3", "py-2"];
const BTN_CLASS: [&str; 8] = [
    "whitespace-nowrap",
    "rounded",
    "px-2",
    "py-1",
    "text-white",
    "font-bold",
    "bg-blue-500",
    "hover:bg-blue-700",
];

// The list of selected reports
type Selected = Mutable<HashSet<ReportID>>;

fn render_report_row(state: &Rc<App>, selected: &Selected, report: &ReportRow) -> Dom {
    let (report_route, name) = match &report.target {
        ReportKind::Similarity => (Route::Similarity(report.id), "similarity"),
        ReportKind::Target(target) => (Route::Report(report.id), target.as_str()),
    };
    let rid = report.id;
    let (m_checkbox, status) = match &report.status {
        ReportStatus::Pending => (
            None,
            if name != "similarity" {
                link!(state.to_url(Route::Watch(report.id)), {.text("watch")})
            } else {
                // These are fast, if needed, improve the Watch route to handle the similarity case
                html!("div", {.text("pending")})
            },
        ),
        ReportStatus::Completed => (
            // The selection checkbox updates the selected list on click
            if name != "similarity" {
                Some(html!("input" => HtmlInputElement, {
                    .attr("type", "checkbox")
                    .with_node!(element => {
                        .event(clone!(selected => move |_: events::Input| {
                            let mut lock = selected.lock_mut();
                            if element.checked() {
                                lock.insert(rid);
                            } else {
                                lock.remove(&rid);
                            }
                        }))
                    })
                }))
            } else {
                None
            },
            link!(state.to_url(report_route), {.text("read")}),
        ),
        ReportStatus::Error(err) => (
            None,
            link!(state.to_url(report_route), {.text("error").attr("title", &err)}),
        ),
    };
    html!("tr", {.class(["border-b", "px-6"]).children(&mut [
        match m_checkbox {
            None => html!("td"),
            Some(dom) => html!("td", {.class(TH_CLASS).child(dom)})
        },
        html!("td", {.class(TH_CLASS).child(status)}),
        html!("td", {.class(TH_CLASS).text(&format!("{}", report.anomaly_count))}),
        html!("td", {.class(TH_CLASS).text(name)}),
        html!("td", {.class(TH_CLASS).text(&report.baseline)}),
        html!("td", {.class(TH_CLASS).text(&format!("{}", report.updated_at))}),
    ])})
}

fn render_report_rows(state: &Rc<App>, selected: &Selected, reports: &[ReportRow]) -> Dom {
    let mut tbody = reports
        .iter()
        .map(|row| render_report_row(state, selected, row))
        .collect::<Vec<Dom>>();
    html!("table", {.class(["w-full", "text-sm", "text-left"]).children(&mut [
        html!("thead", {.class(["bg-slate-100"]).children(&mut [
            html!("th", {.class(TH_CLASS).text("Select")}),
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

/// Render the target form input, adding a buton to add/remove extra target for comparaison.
fn target_input(
    show_submit: &Mutable<bool>,
    targets: &MutableVec<Mutable<String>>,
    url: Mutable<String>,
    first: bool,
) -> Dom {
    let button = if first {
        // The first target has a button to add more target
        html!("div", {.visible_signal(show_submit.signal()).class(BTN_CLASS).text("Compare")
        .event(clone!(targets => move |_: events::Click| {
            targets.lock_mut().push_cloned(Mutable::new("".to_string()));
        }))})
    } else {
        // The other target has a button to remove the current target
        html!("div", {.class(BTN_CLASS).text("Remove")
        .event(clone!(url => clone!(targets => move |_: events::Click| {
            let mut targets = targets.lock_mut();
            if let Some(pos) = targets.iter().rposition(
                |x| x.lock_ref().deref() == url.lock_ref().deref()) {
                if pos > 0 {
                    // Check that we are not removing the top target
                    // when it has the same content as the current one
                    targets.remove(pos);
                }
            }
        })))})
    };
    html!("div", {.class("flex").children(&mut [
        html!("input" => HtmlInputElement, {
            .focused(true)
            .class(["w-full", "rounded", "border", "px-1", "my-1"])
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
        button
    ])})
}

fn render_input(state: &Rc<App>) -> Dom {
    let baseline = Mutable::new("".to_string());
    let show_submit = Mutable::new(false);

    // Store the list of target values (Mutable<String>).
    let targets = MutableVec::new_with_values(vec![Mutable::new("".to_string())]);

    // Render the dom for each target value
    let targets_dom = targets
        .signal_vec_cloned()
        .enumerate()
        .map(clone!(targets => clone!(show_submit => move |url|
                                     target_input(&show_submit, &targets, url.1, url.0.get().unwrap_or(0) == 0))));

    html!("form", {.class("grid").class(["shadow-lg", "rounded-md", "py-3", "px-5"]).children(&mut [
        html!("div", {.children_signal_vec(targets_dom)}),
        html!("div", {.class(["flex", "justify-center", "mt-2", "gap-2"])
                      .visible_signal(show_submit.signal()).children(&mut [
            html!("button", {.class(BTN_CLASS).text("LogJuicer Search")}),
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
    ]).event_with_options(&EventOptions::preventable(), clone!(state => clone!(targets => move |ev : events::Submit| {
        // Form submission logic
        let targets = targets.lock_ref();

        // Get the baseline
        let baseline_url: &str = &(baseline.lock_mut());
        let baseline = if baseline_url.is_empty() || !is_valid_url(baseline_url) {
            None
        } else {
            Some(baseline_url.into())
        };

        if targets.len() == 1 {
            // Only one target, create a new report
            let url = &targets[0];
            let target_url: &str = &(url.lock_mut());
            if is_valid_url(target_url) {
                state.visit(Route::NewReport(target_url.into(), baseline));
            }
        } else {
            // Otherwise make a new similarity report
            let mut targets_str: Vec<Rc<str>> = Vec::new();
            for target in targets.iter() {
                let target_url: &str = &(target.lock_mut());
                targets_str.push(target_url.into());
            }
            state.visit(Route::MakeSimilarity(targets_str, baseline));
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

/// Create all the reports then create the similarity report
pub fn make_similarity(state: &Rc<App>, targets: &[Rc<str>], baseline: &Option<Rc<str>>) -> Dom {
    // Keep track of the completed reports (None when they fails)
    let completed = Mutable::new(HashSet::new());
    let expected = targets.len();

    let update_status = clone!(state => clone!(completed => move |s: &HashSet<Option<ReportID>>| {
        if s.len() == expected {
            let reports = completed
                .lock_ref()
                .iter()
                .flatten()
                .sorted()
                .map(|rid| format!("{rid}"))
                .join(":");
            if reports.find(':').is_some() {
                state.visit(Route::NewSimilarity(reports.into()));
                "completed".to_string()
            } else {
                "Failed to create a similarity report, need at least 2 completed reports".to_string()
            }
        } else {
            format!("Progress {}/{expected}", s.len())
        }
    }));

    html!("div", {.children(&mut [
        html!("div", {.text_signal(completed.signal_ref(update_status))}),
        html!("div", {.children(
            targets.iter().map(
                |target| do_render_tail(state, state.new_report_url(target, baseline.as_deref()), completed.clone())
        ))})
    ])})
}

pub fn do_render_audit(state: &Rc<App>) -> Dom {
    let reports = Mutable::new(None);
    let url = state.reports_url();
    spawn_local(clone!(reports => async move {
        let resp = request_reports(&url).await;
        reports.replace(Some(resp));
    }));

    let selected = Mutable::new(HashSet::new());
    fn render_selected(state: Rc<App>, xs: &HashSet<ReportID>) -> Option<Dom> {
        if xs.is_empty() {
            None
        } else {
            let selections = xs.iter().sorted().map(|rid| format!("{rid}")).join(":");
            Some(
                html!("button", {.class(BTN_CLASS).text(&format!("Compare reports: {}", selections)).event(move |_: events::Click| {
                    state.visit(Route::NewSimilarity(selections.clone().into()));
                })}),
            )
        }
    }

    html!("div", {.class("p-2").children(&mut [
        html!("div", {.class("pb-2").child_signal(selected.signal_ref(clone!(state => move |xs| render_selected(state.clone(), xs))))}),
        html!("div", {.child_signal(reports.signal_ref(clone!(selected => clone!(state => move |reports| Some(match reports {
            Some(Ok(reports)) => render_report_rows(&state, &selected, reports),
            Some(Err(err)) => html!("div", {.children(&mut [text("Error: "), text(err)])}),
            None => html!("div", {.text("loading...")}),
        })))))}),
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

/// do_render_new is a dom component in charge of managing reports creation.
/// Once the report is ready, the route is set to the render route,
/// e.g. to display the report or the similarity report.
pub fn do_render_new<MkRoute>(state: &Rc<App>, new_url: String, render_route: MkRoute) -> Dom
where
    MkRoute: FnOnce(ReportID) -> Route + Copy + 'static,
{
    let result: Mutable<FetchResult<(ReportID, ReportStatus)>> = Mutable::new(None);
    spawn_local(clone!(result => async move {
        let resp = request_new_report(&new_url).await;
        result.replace(Some(resp));
    }));
    html!("div", {.child_signal(result.signal_ref(clone!(state => move |data| match data {
            Some(Ok((report_id, ReportStatus::Pending))) => {
                Some(do_render_run(&state, *report_id, render_route(*report_id)))
            },
            Some(Ok((report_id, ReportStatus::Completed))) => {
                state.replace_url(render_route(*report_id));
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
pub fn do_render_run(state: &Rc<App>, report_id: ReportID, render_route: Route) -> Dom {
    let infos: MutableVec<Rc<String>> = MutableVec::new();
    let url = state.ws_report_url(report_id);
    let mut ws = WebSocket::open(&url).unwrap();

    let handler = clone!(state => clone!(infos => async move {
        // Pull progress message from the websocket
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(msg))) if msg == "Done" => break,
                Some(Ok(Message::Text(msg))) => {
                    infos.lock_mut().push_cloned(Rc::new(msg));
                }
                other => {
                    log!("WebSocket stream ended!: {}", format!("{:?}", other));
                    break
                }
            }
        }
        gloo_timers::future::TimeoutFuture::new(1_000).await;
        state.replace_url(render_route);
    }));

    let sig = infos
        .signal_vec_cloned()
        .map(|ev| html!("pre", {.class(["font-mono", "m-2", "ml-4"]).text(&ev)}));

    html!("div", {.future(handler).class("px-2").children_signal_vec(sig)})
}

/// Create a single report and wait for it's completion
fn do_render_tail(
    state: &Rc<App>,
    url: String,
    completed: Mutable<HashSet<Option<ReportID>>>,
) -> Dom {
    let title_dom =
        html!("p", {.class(["px-1", "py-2", "font-bold", "w-full", "bg-slate-100"]).text(&url)});

    // Request the target status
    let result: Mutable<FetchResult<(ReportID, ReportStatus)>> = Mutable::new(None);
    spawn_local(clone!(completed => clone!(result => async move {
        let resp = request_new_report(&url).await;
        match resp {
            Ok((rid, ReportStatus::Completed)) => {completed.lock_mut().insert(Some(rid));},
            Ok((_, ReportStatus::Error(_))) => {completed.lock_mut().insert(None);},
            Err(_) => {completed.lock_mut().insert(None);},
            Ok((_, ReportStatus::Pending)) => {},
        }
        result.replace(Some(resp));
    })));

    html!("div", {.children(&mut [
        title_dom,
        html!("div", {.child_signal(result.signal_ref(clone!(state => clone!(completed => move |data| match data {
            // The report is in progress, tail it's status
            Some(Ok((report_id, ReportStatus::Pending))) => {
                Some(do_tail(&state, *report_id, completed.clone()))
            },
            Some(Ok((_, ReportStatus::Completed))) => {
                Some(html!("p", {.text("completed")}))
            },
            Some(Ok((_, ReportStatus::Error(e)))) => Some(html!("div", {.children(&mut [
                text("Processing error: "),
                text(e)
            ])})),
            Some(Err(err)) => Some(html!("div", {.children(&mut [text("Error: "), text(err)])})),
            None => Some(html!("div", {.text("loading...")}))
        }))))})
    ])})
}

fn do_tail(
    state: &Rc<App>,
    report_id: ReportID,
    completed: Mutable<HashSet<Option<ReportID>>>,
) -> Dom {
    let infos: MutableVec<Rc<String>> = MutableVec::new();
    let url = state.ws_report_url(report_id);
    let mut ws = WebSocket::open(&url).unwrap();

    let handler = clone!(state => clone!(infos => async move {
            // Pull progress message from the websocket
            loop {
                match ws.next().await {
                    Some(Ok(Message::Text(msg))) if msg == "Done" => break,
                    Some(Ok(Message::Text(msg))) => {
                        infos.lock_mut().push_cloned(Rc::new(msg));
                    }
                    other => {
                        log!("WebSocket stream ended!: {}", format!("{:?}", other));
                        break
                    }
                }
            }
            gloo_timers::future::TimeoutFuture::new(1_000).await;

            // Check the status from the API
            let status_url = state.status_url(report_id);
            match gloo_net::http::Request::get(&status_url).send().await {
                Ok(resp) => {
                    match resp.json().await {
                        Err(err) => {
                            infos.lock_mut().push_cloned(format!("Failed to get the status {err}").into());
                            completed.lock_mut().insert(None);
                        },
                        Ok(status) => {
                            infos.lock_mut().push_cloned(Rc::new(format!("{:?}", status)));
                            match status {
                                ReportStatus::Completed => {completed.lock_mut().insert(Some(report_id));},
                                _ => {completed.lock_mut().insert(None);},
                            }
                        }
                    };
                },
                Err(err) => {
                    infos.lock_mut().push_cloned(format!("Failed to get the status {err}").into());
                    completed.lock_mut().insert(None);
                }
            }
    }));
    let sig = infos
        .signal_vec_cloned()
        .map(|ev| html!("pre", {.class(["font-mono", "m-2", "ml-4"]).text(&ev)}));

    html!("div", {.future(handler).class("px-2").children_signal_vec(sig)})
}
