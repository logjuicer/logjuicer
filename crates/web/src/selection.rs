// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module contains the log line selection logic

pub struct Selection {
    start: usize,
    end: Option<usize>,
}

impl Selection {
    pub fn new(start: usize) -> Selection {
        Selection { start, end: None }
    }

    pub fn mk_id(pos: usize) -> String {
        format!("n{}", pos)
    }

    pub fn parse_id(s: &str) -> Option<usize> {
        s[1..].parse().ok()
    }

    /// Return the center element.
    pub fn elem_id(&self) -> String {
        let pos = match self.end {
            Some(end) => self.start + ((end - self.start) / 2),
            None => self.start,
        };
        Selection::mk_id(pos)
    }

    pub fn set(pos: usize) {
        let selection = Selection::new(pos);
        selection.apply();
    }

    pub fn update(pos: usize) {
        let selection = match Selection::from_url() {
            Some(selection) if pos > selection.start => Selection {
                start: selection.start,
                end: Some(pos),
            },
            _ => Selection::new(pos),
        };
        selection.apply();
    }

    pub fn from_url() -> Option<Selection> {
        let hash = web_sys::window()?.location().hash().ok()?;
        let value = hash.trim_start_matches('#');

        let (start, end) = match value.split_once('-') {
            None => (value.parse().ok()?, None),
            Some((s1, s2)) => (s1.parse().ok()?, Some(s2.parse().ok()?)),
        };
        Some(Selection { start, end })
    }

    fn to_url(&self) {
        // Update the navigation bar
        let history = web_sys::window().unwrap().history().unwrap();
        let state = wasm_bindgen::JsValue::NULL;
        let value = match self.end {
            Some(end) => format!("#{}-{}", self.start, end),
            None => format!("#{}", self.start),
        };
        history
            .push_state_with_url(&state, "", Some(&value))
            .unwrap();
    }

    pub fn highlight(&self) {
        let body = web_sys::window().unwrap().document().unwrap();
        for pos in self.start..=self.end.unwrap_or(self.start) {
            let elem_id = Selection::mk_id(pos);
            if let Some(elem) = body.get_element_by_id(&elem_id) {
                highlight_row(&elem)
            }
        }
    }

    pub fn apply(&self) {
        clear_selection();
        self.highlight();
        self.to_url();
    }
}

const SELECTION_HIGHLIGHT: &str = "bg-amber-100";

fn clear_selection() {
    let body = dominator::body();
    let elems = body.get_elements_by_class_name(SELECTION_HIGHLIGHT);

    for pos in (0..elems.length()).rev() {
        if let Some(elem) = elems.item(pos) {
            let _ = elem.class_list().remove_1(SELECTION_HIGHLIGHT);
        }
    }
}

fn highlight_row(elem: &web_sys::Element) {
    if let Some(parent) = elem.parent_element() {
        let _ = parent.class_list().add_1(SELECTION_HIGHLIGHT);
    }
}

use gloo_console::log;
use gloo_timers::future::TimeoutFuture;
// This function waits for the hash to be rendered
pub async fn put_hash_into_view(selection: Selection) {
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
