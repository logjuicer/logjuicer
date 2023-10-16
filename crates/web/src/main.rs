// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module is the entrypoint of the logreduce web interface.

use dominator::{clone, html, link, routing, Dom};
use futures_signals::signal::SignalExt;
use gloo_console::log;
use std::rc::Rc;

mod selection;

mod dom_utils;
use dom_utils::{data_attr, render_link};

mod dom_report;
use dom_report::{fetch_and_render_report, render_report_card};

mod dom_api;
use dom_api::*;

mod state;
use state::{App, Route};

fn render_app(state: &Rc<App>) -> Dom {
    let about = html!("div", {.class(["tooltip", "top-1"]).children(&mut [
        html!("p", {.class("text-gray-700").text("This is logreduce report viewer.")}),
        html!("div", {.class(["hover:bg-slate-400"]).children(&mut [
            render_link("https://github.com/logreduce/logreduce#readme", "documentation")
        ])}),
        data_attr("Viewer", env!("CARGO_PKG_VERSION")),
        data_attr("License", env!("CARGO_PKG_LICENSE")),
    ])});
    let router = routing::url()
        .signal_ref(|url| state::Route::from_url(url))
        .for_each(clone!(state => move |route| {
            state.route.set_neq(route);
            async {}
        }));
    let backlink = if state.is_static {
        html!("span", {.text("logreduce")})
    } else {
        link!(state.base_path.clone(), {.text("logreduce")})
    };
    html!("div", {.future(router).children(&mut [
        html!("nav", {.class(["sticky", "top-0", "bg-slate-300", "z-50", "flex", "px-1", "divide-x"]).children(&mut [
            html!("div", {.class("grow").children(&mut [backlink])}),
            html!("div", {.class(["has-tooltip", "flex", "items-center"])
                          .child_signal(state.report.signal_ref(|data| match data {
                              Some(Ok(report)) => Some(html!("div", {.children(&mut [
                                  render_report_card(report),
                                  html!("div", {.class(["px-2", "text-sm"]).text("info")}),
                              ])})),
                              _ => None
                          }))}),
            html!("div", {.class(["has-tooltip", "px-2", "flex", "items-center"]).children(&mut [
                about,
                html!("div", {.class("text-sm").text("about")}),
            ])})
        ])}),
    ]).child_signal(state.route.signal_ref(clone!(state => move |route| Some(match route {
        Route::StaticViewer => fetch_and_render_report(&state, "report.bin".into()),
        Route::Report(report_id) => fetch_and_render_report(&state, state.report_url(*report_id)),
        Route::NewReport(target) => do_render_new(&state, state.new_report_url(target)),
        Route::Watch(report_id) => do_render_run(&state, *report_id),
        Route::Welcome => do_render_welcome(&state),
    }))))})
}

// use futures::{SinkExt, StreamExt};
// use gloo_net::websocket::futures::WebSocket;
pub fn main() {
    console_error_panic_hook::set_once();
    let app = App::new();
    log!("Rendering the app!");
    dominator::append_dom(&dominator::body(), render_app(&app));
}
