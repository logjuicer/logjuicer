// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use dominator::routing;
use futures_signals::signal::Mutable;
use logreduce_report::report_row::ReportID;
use logreduce_report::Report;
use std::rc::Rc;
use std::str::FromStr;
use web_sys::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Report(ReportID),
    Watch(ReportID),
    NewReport(Rc<str>),
    Welcome,
}

impl Route {
    pub fn from_url(url_str: &str) -> Self {
        let url = Url::new(url_str).unwrap();
        let path = url.pathname();
        let params = url.search_params();
        if path.ends_with("/report/new") {
            if let Some(target) = params.get("target") {
                Route::NewReport(target.into())
            } else {
                Route::Welcome
            }
        } else {
            let components = path.rsplit('/').collect::<Vec<&str>>();
            match components[..] {
                [report_id_str, "watch", "report", ..] => match ReportID::from_str(report_id_str) {
                    Ok(report_id) => Route::Watch(report_id),
                    Err(_) => Route::Welcome,
                },
                [report_id_str, "report", ..] => match ReportID::from_str(report_id_str) {
                    Ok(report_id) => Route::Report(report_id),
                    Err(_) => Route::Welcome,
                },
                _ => Route::Welcome,
            }
        }
    }

    pub fn to_url(&self, base: &str) -> String {
        match self {
            Route::NewReport(target) => format!("{}report/new?target={}", base, target),
            Route::Watch(report_id) => format!("{}report/watch/{}", base, report_id),
            Route::Report(report_id) => format!("{}report/{}", base, report_id),
            _ => format!("{}logreduce.html", base),
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
    pub base_path: Box<str>,
    pub ws_api: Box<str>,
}

impl App {
    pub fn visit(&self, route: Route) {
        let route_url = self.to_url(route);
        dominator::routing::go_to_url(&route_url);
    }

    pub fn replace_url(&self, route: Route) {
        let route_url = self.to_url(route);
        dominator::routing::replace_url(&route_url);
    }

    pub fn to_url(&self, route: Route) -> String {
        route.to_url(&self.base_path)
    }

    pub fn reports_url(&self) -> String {
        format!("{}api/reports", self.base_path)
    }

    pub fn report_url(&self, report_id: ReportID) -> String {
        format!("{}api/report/{}", self.base_path, report_id)
    }

    pub fn new_report_url(&self, target: &str) -> String {
        format!("{}api/report/new?target={}", self.base_path, target)
    }

    pub fn ws_report_url(&self, report_id: ReportID) -> String {
        format!("{}report/{}", self.ws_api, report_id)
    }

    pub fn new() -> Self {
        let binding = routing::url();
        // Figure out what is the initial route
        let initial_url = binding.lock_ref();
        let initial_route = Route::from_url(&initial_url);

        let base_path = if initial_url.contains("/logreduce/") {
            "/logreduce/".into()
        } else {
            "/".into()
        };

        let url = Url::new(&initial_url).unwrap();
        let ws_proto = if url.protocol() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let ws_api = format!("{}://{}{}wsapi/", ws_proto, url.host(), base_path).into();
        gloo_console::log!(format!("Initial ws_api: {}", ws_api));
        Self {
            report: Mutable::new(None),
            route: Mutable::new(initial_route),
            base_path,
            ws_api,
        }
    }
}
