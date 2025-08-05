// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use crate::dom_utils::ReportMode;
use dominator::routing;
use futures_signals::signal::Mutable;
use logjuicer_report::report_row::ReportID;
use logjuicer_report::Report;
use std::rc::Rc;
use std::str::FromStr;
use web_sys::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    // Display a report
    Report(ReportID),
    // Display a similarity report
    Similarity(ReportID),
    // Watch a report (from the audit page)
    Watch(ReportID),
    // Request a new report
    NewReport(Rc<str>, NewReportKind),
    // Request a new similarity report
    NewSimilarity(Rc<str>),
    // Make a new similary report
    MakeSimilarity(Vec<Rc<str>>, Option<Rc<str>>),
    // The welcome page
    Welcome,
    // The audit page
    Audit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NewReportKind {
    NoBaseline,
    Errors,
    NoErrors,
    Baseline(Rc<str>),
}

impl Route {
    pub fn from_url(url_str: &str) -> Self {
        let url = Url::new(url_str).unwrap();
        let path = url.pathname();
        let params = url.search_params();
        if path.ends_with("/report/new") {
            let report_kind = match (params.get("errors"), params.get("baseline")) {
                (Some(e), _) if &e == "true" => NewReportKind::Errors,
                (Some(e), _) if &e == "false" => NewReportKind::NoErrors,
                (_, Some(baseline)) => NewReportKind::Baseline(baseline.into()),
                (_, None) => NewReportKind::NoBaseline,
            };
            if let Some(target) = params.get("target") {
                Route::NewReport(target.into(), report_kind)
            } else {
                Route::Welcome
            }
        } else if path.ends_with("/similarity/new") {
            if let Some(reports) = params.get("reports") {
                Route::NewSimilarity(reports.into())
            } else {
                Route::Welcome
            }
        } else if path.ends_with("/similarity/make") {
            let baseline = params.get("baseline");
            if let Some(targets) = params.get("targets") {
                let targets = targets.split(',').map(|s| s.into()).collect();
                Route::MakeSimilarity(targets, baseline.map(|s| s.into()))
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
                [report_id_str, "similarity", ..] => match ReportID::from_str(report_id_str) {
                    Ok(report_id) => Route::Similarity(report_id),
                    Err(_) => Route::Welcome,
                },
                [report_id_str, "report", ..] => match ReportID::from_str(report_id_str) {
                    Ok(report_id) => Route::Report(report_id),
                    Err(_) => Route::Welcome,
                },
                ["audit", ..] => Route::Audit,
                _ => Route::Welcome,
            }
        }
    }

    pub fn to_url(&self, base: &str) -> String {
        match self {
            Route::NewReport(target, NewReportKind::NoBaseline) => {
                format!("{}report/new?target={}", base, target)
            }
            Route::NewReport(target, NewReportKind::Errors) => {
                format!("{}report/new?target={}&errors=true", base, target)
            }
            Route::NewReport(target, NewReportKind::NoErrors) => {
                format!("{}report/new?target={}&errors=false", base, target)
            }
            Route::NewReport(target, NewReportKind::Baseline(baseline)) => {
                format!("{}report/new?target={}&baseline={}", base, target, baseline)
            }
            Route::NewSimilarity(reports) => {
                format!("{}similarity/new?reports={reports}", base)
            }
            Route::MakeSimilarity(targets, None) => {
                format!("{}similarity/make?targets={}", base, targets.join(","))
            }
            Route::MakeSimilarity(targets, Some(baseline)) => {
                format!(
                    "{}similarity/make?targets={}&baseline={}",
                    base,
                    targets.join(","),
                    baseline
                )
            }
            Route::Watch(report_id) => format!("{}report/watch/{}", base, report_id),
            Route::Report(report_id) => format!("{}report/{}", base, report_id),
            Route::Similarity(report_id) => format!("{}similarity/{}", base, report_id),
            Route::Audit => format!("{}audit", base),
            Route::Welcome => base.to_string(),
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
    pub report: Mutable<Option<Result<(Report, ReportMode), String>>>,
    pub route: Mutable<Route>,
    pub base_path: Box<str>,
    pub ws_api: Box<str>,
}

impl App {
    pub fn clear_report(&self) {
        self.report.set_neq(None)
    }

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

    pub fn status_url(&self, report_id: ReportID) -> String {
        format!("{}api/report/{}/status", self.base_path, report_id)
    }

    pub fn new_report_url(&self, target: &str, report_kind: &NewReportKind) -> String {
        let base = &self.base_path;
        match report_kind {
            NewReportKind::NoBaseline => format!("{base}api/report/new?target={target}"),
            NewReportKind::Baseline(baseline) => {
                format!("{base}api/report/new?target={target}&baseline={baseline}")
            }
            NewReportKind::NoErrors => format!("{base}api/report/new?target={target}&errors=false"),
            NewReportKind::Errors => format!("{base}api/report/new?target={target}&errors=true"),
        }
    }

    pub fn new_similarity_url(&self, reports: &str) -> String {
        let base = &self.base_path;
        format!("{base}api/similarity/new?reports={reports}")
    }

    pub fn ws_report_url(&self, report_id: ReportID) -> String {
        format!("{}report/{}", self.ws_api, report_id)
    }

    pub fn new() -> Self {
        let binding = routing::url();
        // Figure out what is the initial route
        let initial_url = binding.lock_ref();
        let initial_route = Route::from_url(&initial_url);

        let base_path = if initial_url.contains("/logjuicer/") {
            "/logjuicer/".into()
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
