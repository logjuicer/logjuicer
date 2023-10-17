// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use dominator::{html, Dom};

pub fn render_link(href: &str, text: &str) -> Dom {
    html!("a", {.class("cursor-pointer").attr("href", href).attr("target", "_black").text(text)})
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
