// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use dominator::{html, Dom};

pub fn mk_card(title: &str, id: &str, body: Dom) -> Dom {
    html!("div", {.class(["mt-2"]).children(&mut [
        html!("div", {.class(["bg-slate-100", "w-full", "font-semibold", "pl-2"]).text(title).attr("id", id)}),
        body
    ])})
}

pub struct ReportAndBaselines {
    pub data: Vec<u8>,
    pub baselines: Option<String>,
}

#[derive(PartialEq)]
pub enum ReportMode {
    Auto,
    NotAuto,
}

pub async fn fetch_data(path: &str) -> Result<ReportAndBaselines, String> {
    let resp = gloo_net::http::Request::get(path)
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;
    if resp.status() == 404 {
        let msg = resp.text().await;
        match msg {
            Ok(msg) => Err(msg),
            Err(e) => Err(format!("Not found {}", e)),
        }
    } else if !resp.ok() {
        Err(format!("Bad status: {}", resp.status()))
    } else {
        let data = resp
            .binary()
            .await
            .map_err(|e| format!("Response error: {}", e))?;
        let baselines = resp.headers().get("x-baselines");
        Ok(ReportAndBaselines { data, baselines })
    }
}

pub fn render_link(href: &str, text: &str) -> Dom {
    if href.starts_with("http") {
        html!("a", {.class("external").attr("href", href).attr("target", "_blank").text(text)})
    } else {
        html!("a", {.class("internal").attr("href", href).text(text)})
    }
}

pub fn data_attr_html(name: &str, value: &mut [Dom]) -> Dom {
    html!("div", {.class("flex").children(&mut [
        html!("dt", {.class(["w-32", "font-medium", "text-gray-900"]).text(name)}),
        html!("dd", {.class(["flex", "items-center", "text-sm", "text-gray-700", "sm:col-span-5", "sm:mt-0"]).children(value)})
    ])})
}

pub fn data_attr(name: &str, value: &str) -> Dom {
    data_attr_html(name, &mut [dominator::text(value)])
}

#[cfg(feature = "api_client")]
pub type FetchResult<Value> = Option<Result<Value, String>>;

/// A state used for rendering purpose
#[derive(Default)]
pub struct RenderState {
    /// The global line position for the anchor selector
    pub gl_pos: usize,
    /// The set of already displayed anomalies
    pub uniques: HashSet<Arc<str>>,
}
