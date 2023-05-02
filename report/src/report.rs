// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use html_builder::*;
use itertools::Itertools;
use std::borrow::Cow;
use std::fmt::Write;

type Result<A> = core::result::Result<A, std::fmt::Error>;

pub fn render(report: &logreduce_model::Report) -> Result<String> {
    Ok(Html::from(report)?.render())
}

struct Html {
    buffer: Buffer,
}

impl Html {
    fn from(report: &logreduce_model::Report) -> Result<Html> {
        let mut buffer = Buffer::new();
        let mut html = buffer.html().attr("lang='en'");

        add_head(&mut html, &format!("Logreduce of {}", report.target))?;
        add_body(&mut html, report)?;

        Ok(Html { buffer })
    }
}

impl Html {
    fn render(self) -> String {
        self.buffer.finish()
    }
}

fn table(parent: &mut Node, columns: Option<&[&str]>, rows: &[&[&str]]) -> Result<()> {
    let mut div = parent
        .div()
        .attr("id='debuginfo'")
        .attr("style='overflow-x: auto'");
    let mut table = div
        .table()
        .attr("style='white-space: nowrap; margi: 0px'")
        .attr("class='table table-condensed table-responsive table-bordered'");
    if let Some(columns) = columns {
        let mut thead = table.thead();
        let mut tr = thead.tr();
        for column in columns {
            tr.th().write_str(column)?;
        }
    }
    let mut tbody = table.tbody();
    for row in rows.iter() {
        let mut tr = tbody.tr();
        for cell in row.iter() {
            tr.td().write_str(cell)?;
        }
    }
    Ok(())
}

fn add_head(parent: &mut Node, title: &str) -> Result<()> {
    fn add_link(head: &mut Node, href: &str, integrity: &str) {
        head.link()
            .attr(&format!("href=\"{}\"", href))
            .attr("rel=\"stylesheet\"")
            .attr(&format!("integrity=\"{}\"", integrity))
            .attr("crossorigin=\"anonymous\"");
    }
    let mut head = parent.head();
    head.title().write_str(title)?;
    head.meta().attr("charset='utf-8'");
    for (href, integrity) in STYLES {
        add_link(&mut head, href, integrity);
    }
    head.style().write_str(include_str!("style.css"))?;
    Ok(())
}

fn add_body(parent: &mut Node, report: &logreduce_model::Report) -> Result<()> {
    fn add_script(body: &mut Node, href: &str, integrity: &str) {
        body.script()
            .attr(&format!("src=\"{}\"", href))
            .attr(&format!("integrity=\"{}\"", integrity))
            .attr("crossorigin=\"anonymous\"");
    }

    let mut body = parent.body();

    add_nav(&mut body)?;
    add_container(&mut body, report)?;

    for (src, integrity) in SCRIPTS {
        add_script(&mut body, src, integrity)
    }
    body.script().write_str(JS)?;
    Ok(())
}

fn class_<'b>(node: &'b mut Node<'b>, tag: Cow<'static, str>, class: &str) -> Node<'b> {
    node.child(tag).attr(&format!("class=\"{}\"", class))
}
fn div_<'b>(node: &'b mut Node<'b>, class: &str) -> Node<'b> {
    class_(node, std::borrow::Cow::Borrowed("div"), class)
}

fn add_nav(body: &mut Node) -> Result<()> {
    let mut nav = body
        .nav()
        .attr("class=\"navbar navbar-default navbar-pf\"")
        .attr("role=\"navigation\"");
    let mut header = nav.div().attr("class=\"navbar-header\"");
    header.img().attr(LOGO).attr("alt=\"LogReduce\"");
    let mut div = div_(&mut nav, "collapse navbar-collapse navbar-collapse-1");
    {
        // Utility
        let mut utils = div.ul().attr("class=\"nav navbar-nav navbar-utility\"");
        {
            let mut li = utils.li();
            li.a()
                .attr("href=\"#\"")
                .attr("id='debugbtn'")
                .write_str("Show Debug")?;
        }
        {
            let mut li = utils.li();
            li.a()
                .attr("href=\"https://github.com/logreduce/logreduce\"")
                .attr("target=\"_blank\"")
                .write_str("Documentation")?;
        }
        {
            let mut li = utils.li();
            let mut a = li.a().attr("href=\"#\"");
            a.strong().write_str("Version")?;
            a.write_str(env!("CARGO_PKG_VERSION"))?;
        }
    }
    {
        // Primary
        let mut primary = div.ul().attr("class=\"nav navbar-nav navbar-primary\"");
        {
            let mut li = primary.li().attr("class=\"active\"");
            li.a()
                .attr("href=\"log-classify.html\"")
                .write_str("Report")?;
        }
        {
            let mut li = primary.li();
            li.a().attr("href=\"./\"").write_str("Job Artifacts")?;
        }
    }
    Ok(())
}

fn add_container(body: &mut Node, report: &logreduce_model::Report) -> Result<()> {
    let mut div = body
        .div()
        .attr("class=\"container\"")
        .attr("style='width: 100%'");

    // Info table
    // TODO: reproducer command, baselines info, target info, anomalies count and runtime
    table(
        &mut div,
        None,
        &[
            &["Target", &format!("{}", report.target)],
            &[
                "Baselines",
                &format!("{}", report.baselines.iter().format(", ")),
            ],
            &["Created at", &render_time(&report.created_at)],
            &[
                "Run time",
                &format!("{:.2} sec", report.run_time.as_secs_f32()),
            ],
            &[
                "Result",
                &format!(
                    "{:02.2}% reduction (from {} to {})",
                    (100.0
                        - (report.total_anomaly_count as f32 / report.total_line_count as f32)
                            * 100.0),
                    report.total_line_count,
                    report.total_anomaly_count
                ),
            ],
        ],
    )?;

    // Summary table
    // TODO: Anomaly count | Filename | Test time | Model

    {
        let mut list_group = div_(&mut div, "list-group list-view-pf list-view-pf-view");
        let mut expand = true;
        for log_report in &report.log_reports {
            render_content_report(
                &mut list_group,
                log_report,
                report.index_reports.get(&log_report.index_name),
                expand,
            )?;
            expand = false;
        }
    }

    // Model summary table
    // TODO: Model | Train time | Infos | Baseline files

    // Error table
    // TODO: Add files that were not processed dut to errors or missing model name
    Ok(())
}

fn render_content_report(
    list_group: &mut Node,
    log_report: &logreduce_model::LogReport,
    index_report: Option<&logreduce_model::IndexReport>,
    expand: bool,
) -> Result<()> {
    let mut list_group_item = list_group
        .div()
        .attr(&format!(
            "class=\"list-group-item{}\"",
            if expand {
                " list-view-pf-expand-active"
            } else {
                ""
            }
        ))
        .attr(&format!("id=\"{}\"", "TODO"));

    {
        let mut item_header = list_group_item
            .div()
            .attr("class=\"list-group-item-header\"");
        {
            let mut item_header_expand = item_header.div().attr("class=\"list-view-pf-expand\"");
            item_header_expand.span().attr(&format!(
                "class=\"fa fa-angle-right{}\"",
                if expand { " fa-angle-down" } else { "" }
            ));
        }
        {
            let mut main_info = item_header.div().attr("class=\"list-view-pf-main-info\"");
            {
                let mut pf_left = main_info.div().attr("class=\"list-view-pf-left\"");
                pf_left
                    .span()
                    .attr("class=\"fa pficon-degraded list-view-pf-icon-sm\"");
            }
            {
                let mut pf_body = main_info.div().attr("class=\"list-view-pf-body\"");
                {
                    let mut desc = pf_body.div().attr("class=\"list-view-pf-description\"");
                    desc.div()
                        .attr("class=\"list-group-item-heading\"")
                        .write_str(log_report.source.get_relative())?;
                }

                {
                    let mut additional_item = pf_body
                        .div()
                        .attr("class=\"list-view-pf-additional-info-item\"")
                        .attr("id='debuginfo'");
                    additional_item
                        .span()
                        .attr("class=\"pficon pficon-registry\"");
                    additional_item
                        .a()
                        .attr(&format!(
                            "href=\"{}\"",
                            model_anchor(&log_report.index_name)
                        ))
                        .write_str(&format!("{}", log_report.index_name))?;
                    additional_item.write_str(" model")?;
                }

                {
                    let mut additional_item = pf_body
                        .div()
                        .attr("class=\"list-view-pf-additional-info-item\"");
                    additional_item.span().attr("class=\"fa fa-external-link\"");
                    additional_item
                        .a()
                        .attr(&format!("href=\"{}\"", log_report.source.as_str()))
                        .write_str("file")?;
                }

                {
                    let mut additional_item = pf_body
                        .div()
                        .attr("class=\"list-view-pf-additional-info-item\"");
                    additional_item.span().attr("class=\"fa fa-bug\"");
                    additional_item
                        .strong()
                        .write_str(&format!("{}", log_report.anomalies.len()))?;
                }
            }
        }
    }
    {
        let mut item_container = list_group_item.div().attr(&format!(
            "class=\"list-group-item-container container-fluid{}\"",
            if expand { "" } else { " hidden" }
        ));
        {
            let mut close_icon = item_container.div().attr("class=\"close\"");
            close_icon.span().attr("class=\"pficon pficon-close\"");
        }

        if let Some(index_report) = index_report {
            let mut div = item_container.div().attr("id='debuginfo'");
            div.write_str("Baseline samples:")?;
            let mut ul = div.ul();
            for source in index_report.sources.iter().take(3) {
                ul.li()
                    .a()
                    .attr(&format!("href=\"{}\"", source.as_str()))
                    .write_str(source.as_str())?
            }
        }

        let mut loglines = item_container.div().attr("class=\"loglines\"");
        render_lines(&mut loglines, &log_report.anomalies)?;
    }
    Ok(())
}

fn model_anchor(index_name: &logreduce_model::IndexName) -> String {
    format!("#model_{}", index_name)
}

fn render_context(loglines: &mut Node, pos: usize, xs: &[String]) -> Result<()> {
    for (idx, line) in xs.iter().enumerate() {
        loglines
            .pre()
            .write_str(&format!("   {:4} | {}", pos + 1 + idx, line))?;
    }
    Ok(())
}

fn render_lines(loglines: &mut Node, anomalies: &[logreduce_model::AnomalyContext]) -> Result<()> {
    let mut last_pos = None;

    for anomaly in anomalies {
        let starting_pos = anomaly.anomaly.pos - 1 - anomaly.before.len();
        if let Some(last_pos) = last_pos {
            if last_pos != starting_pos {
                loglines.hr().attr("class=\"ls\"");
            }
        }
        let dist: usize = (anomaly.anomaly.distance * 99.0) as _;
        let color: usize = (anomaly.anomaly.distance * 255.0) as _;

        render_context(loglines, starting_pos, &anomaly.before)?;

        loglines
            .pre()
            .attr(&format!("style=\"color: #{:2X}0000\"", color))
            .write_str(&format!(
                "{:02} {:4} | {}",
                dist, anomaly.anomaly.pos, anomaly.anomaly.line
            ))?;

        render_context(loglines, anomaly.anomaly.pos, &anomaly.after)?;

        last_pos = Some(anomaly.anomaly.pos + anomaly.after.len());
    }

    Ok(())
}

fn render_time(system_time: &std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::offset::Utc> = (*system_time).into();
    datetime.format("%Y-%m-%d %T").to_string()
}

const SCRIPTS: &[(&str, &str)] = &[
    (
        "https://code.jquery.com/jquery-3.3.1.min.js",
        "sha384-tsQFqpEReu7ZLhBV2VZlAu7zcOV+rXbYlF2cqB8txI/8aZajjp4Bqd+V6D5IgvKT",
    ),
    (
        "https://maxcdn.bootstrapcdn.com/bootstrap/3.3.7/js/bootstrap.min.js",
        "sha384-Tc5IQib027qvyjSMfHjOMaLkfuWVxZxUPnCJA7l2mCWNIpG9mGCD8wGNIcPD7Txa",
    ),
    (
        "https://cdnjs.cloudflare.com/ajax/libs/patternfly/3.24.0/js/patternfly.min.js",
        "sha384-4lW8IOzfCHwzlShlG/AP2aYc91C+r+v3lpUEpjBM+FOHy0INgLKgyMBZd7O9ejln",
    ),
];

const STYLES: &[(&str, &str)] = &[
    (
        "https://cdnjs.cloudflare.com/ajax/libs/patternfly/3.24.0/css/patternfly.min.css",
        "sha384-nJHx77dqIJ9kzFQqUhPYDKwrwgp5od9/BrMUGQLygWcJu64ODr9qYWLHQ4K/YxpB",
    ),
    (
        "https://cdnjs.cloudflare.com/ajax/libs/patternfly/3.24.0/css/patternfly-additions.min.css",
        "sha384-q1wvnTYo0F4qq2mYrUz3DfnaM3gXSkyHfVoGbl4zKuZ9SMlg/JIWBHeU398mRAp4",
    ),
];

/// A helper script to make the file list toggleable
static JS: &str = r#"
$(document).ready(function(){
$('#debugbtn').on('click', function(event) {$('[id=debuginfo]').toggle();});
});
$(".list-group-item-header").click(function(event){
  if(!$(event.target).is("button, a, input, .fa-ellipsis-v")){
    $(this).find(".fa-angle-right").toggleClass("fa-angle-down")
      .end().parent().toggleClass("list-view-pf-expand-active")
      .find(".list-group-item-container").toggleClass("hidden");
    }
})
$(".list-group-item-container .close").on("click", function (){
  $(this).parent().addClass("hidden")
         .parent().removeClass("list-view-pf-expand-active")
         .find(".fa-angle-right").removeClass("fa-angle-down");
})
"#;

static LOGO: &str = concat!(
    "src=\"data:image/jpeg;base64,",
    "iVBORw0KGgoAAAANSUhEUgAAABcAAAAXBAMAAAASBMmTAAAAFVBMVEU6feU9geVjl+WMsOUicOXA",
    "z+X///9aF/8vAAAAAXRSTlMAQObYZgAAAAFiS0dEAIgFHUgAAAAJcEhZcwAACxMAAAsTAQCanBgA",
    "AAAHdElNRQfiCAsFFSh04lDsAAAAp0lEQVQY012QXQ7CQAiE158LyKYH6BDf7WAPYEkP4BLvfxVx",
    "WxPjhge+sAwDpfy9oxgfX5hQhbctt0VV4Z1OjTVC+OowtqeNrekH7rO7qq/3/HcAZoIzOZZy5uDt",
    "IrHKlEN0Yg9D9kugQkVbTaDxMkAM9lOJuvWAhinwkR782dVS+iAwjpqzsDlYr7uDcsKPt3TtEc7o",
    "+8Siqeb7doQI9jwFjcv/YcobPpYhOB4CZRcAAAAASUVORK5CYII=",
    "\""
);
