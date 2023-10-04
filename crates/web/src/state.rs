// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use dominator::routing;
use futures_signals::signal::Mutable;
use logreduce_report::Report;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Report,
    Run,
    Welcome,
}

impl Route {
    pub fn from_url(with_hash: &str) -> Self {
        let url = if let Some((base, _)) = with_hash.rsplit_once("#") {
            base
        } else {
            with_hash
        };
        if url.ends_with("/logreduce.html") {
            Route::Report
        } else if url.ends_with("/run") {
            Route::Run
        } else {
            Route::Welcome
        }
    }

    pub fn to_url(&self) -> &str {
        match self {
            _ => "/logreduce.html",
        }
    }
}

impl Default for Route {
    fn default() -> Self {
        // Create the Route based on the current URL
        Self::from_url(&routing::url().lock_ref())
    }
}

pub struct App {
    pub report: Mutable<Option<Result<Report, String>>>,
    pub route: Mutable<Route>,
    pub base_path: Option<Rc<str>>,
}

impl App {
    pub fn new() -> Rc<Self> {
        let binding = routing::url();
        // Figure out what is the initial route
        let initial_url = binding.lock_ref();
        let initial_route = Route::from_url(&initial_url);

        let base_path = if initial_route == Route::Report {
            None // Legacy UI, no navigation
        } else if initial_url.contains("/logreduce/") {
            Some("/logreduce".into())
        } else {
            Some("/".into())
        };

        Rc::new(Self {
            report: Mutable::new(None),
            route: Mutable::new(initial_route),
            base_path,
        })
    }
}
