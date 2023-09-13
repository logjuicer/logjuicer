use gloo_console::log;
use gloo_net::http::Request;
use std::ops::Deref;
use yew::prelude::*;

use logreduce_report::{bytes_to_mb, Content, IndexName, LogReport, Report, Source};

fn data_attr_html(name: &str, value: Html) -> Html {
    html! {
        <div class={classes!("sm:grid", "sm:grid-cols-6", "sm:gap-4", "sm:px-0")}>
            <dt class={classes!("text-sm", "font-medium", "text-gray-900")}>{name}</dt>
            <dd class={classes!("flex", "items-center", "text-sm", "text-gray-700", "sm:col-span-5", "sm:mt-0")}>{value}</dd>
        </div>
    }
}

fn data_attr(name: &str, value: &str) -> Html {
    data_attr_html(name, value.into())
}

fn render_time(system_time: &std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::offset::Utc> = (*system_time).into();
    datetime.format("%Y-%m-%d %T").to_string()
}

static COLORS: &[&str] = &["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9"];

fn render_content(content: &Content) -> Html {
    match content {
        Content::Zuul(zuul_build) => html! {
            <div><a href={zuul_build.build_url()} class="cursor-pointer">{&format!("zuul<job={}, project={}, branch={}, result={}>", zuul_build.job_name, zuul_build.project, zuul_build.branch, zuul_build.result)}</a></div>
        },
        _ => html! {<div>{content.to_string()}</div>},
    }
}

fn render_line(pos: usize, distance: f32, line: &str) -> Html {
    let sev = (distance * 10.0).round() as usize;
    let color: &str = COLORS.get(sev).unwrap_or(&"c0");
    html! {
        <tr>
            <td class={classes!("pos")}>{pos}</td>
            <td class={classes!("pl-2", "break-all", color)}>{line}</td>
        </tr>
    }
}

fn log_name(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

fn render_log_report(report: &Report, log_report: &LogReport) -> Html {
    let index_name = &format!("{}", log_report.index_name);
    let info_index = match report.index_reports.get(&log_report.index_name) {
        Some(index_report) => {
            let sources: Html = index_report
                .sources
                .iter()
                .map(|source| html! {<div><a href={source.as_str().to_string()}>{log_name(source.get_relative())}</a></div>})
                .collect();
            html! {
                <>
                {data_attr_html("Baselines", html!{<div>{sources}</div>})}
                {data_attr("Index", index_name)}
                {data_attr("Training time", &format!("{} ms", index_report.train_time.as_millis()))}
                </>
            }
        }
        None => html! {data_attr("Unknown Index", index_name)},
    };
    let info_btn = html! {
      <div class={classes!("has-tooltip", "px-2")}>
        <div class={classes!("tooltip")}>
            {info_index}
            {data_attr("Test time", &format!("{} ms", log_report.test_time.as_millis()))}
            {data_attr("Anomaly count", &format!("{}", log_report.anomalies.len()))}
            {data_attr("Log size", &format!("{} lines, {:.3} MB", log_report.line_count, bytes_to_mb(log_report.byte_count)))}
        </div>
        <div class={classes!("font-bold", "text-slate-500")}>{"?"}</div>
      </div>
    };
    let header = html! {
        <div class={classes!("bg-slate-100", "flex", "divide-x", "mr-2")}>
          <div class={classes!("grow", "flex")}>
            <a href={log_report.source.as_str().to_string()} target_="_black">{ log_report.source.get_relative() }</a>
          </div>
          {info_btn}
        </div>
    };
    let mut lines = Vec::with_capacity(log_report.anomalies.len() * 2);
    for anomaly in &log_report.anomalies {
        for (pos, line) in anomaly.before.iter().enumerate() {
            let prev_pos = anomaly
                .anomaly
                .pos
                .saturating_sub(anomaly.before.len() - pos);
            lines.push(render_line(prev_pos, 0.0, line));
        }
        lines.push(render_line(
            anomaly.anomaly.pos,
            anomaly.anomaly.distance,
            &anomaly.anomaly.line,
        ));
        for (pos, line) in anomaly.after.iter().enumerate() {
            lines.push(render_line(anomaly.anomaly.pos + 1 + pos, 0.0, line));
        }
    }
    html! {
        <div class={classes!("pl-1", "pt-2", "relative", "max-w-full")}>
            {header}
            <table class={classes!("font-mono")}>
              <thead><tr><th class={classes!("w-12", "min-w-[3rem]")}></th><th></th></tr></thead>
              <tbody>
                {lines}
              </tbody>
            </table>
        </div>
    }
}

fn render_error(source: &Source, body: Html) -> Html {
    html! {
        <div class={classes!("pl-1", "pt-2", "relative", "max-w-full")}>
            <div class={classes!("bg-red-100")}>
                {source.as_str()}
            </div>
            <div>
                {body}
            </div>
        </div>
    }
}

fn render_log_error(source: &Source, error: &str) -> Html {
    render_error(source, html! {<>{"Read failure: "}{error}</>})
}

fn render_unknown(index: &IndexName, sources: &[Source]) -> Html {
    sources
        .iter()
        .map(|source| {
            render_error(
                source,
                html! {<>{"Unknown file, looked for index: "}{index}</>},
            )
        })
        .collect()
}

fn render_report(report: &Report) -> Html {
    let result = format!(
        "{:02.2}% reduction (from {} to {})",
        (100.0 - (report.total_anomaly_count as f32 / report.total_line_count as f32) * 100.0),
        report.total_line_count,
        report.total_anomaly_count
    );
    let card = html! {
        <dl class={classes!("divide-y", "divide-gray-100", "pl-4")}>
            {data_attr_html("Target",    render_content(&report.target))}
            {data_attr_html("Baselines", report.baselines.iter().map(render_content).collect::<Html>())}
            {data_attr("Created at", &render_time(&report.created_at))}
            {data_attr("Run time",   &format!("{:.2} sec", report.run_time.as_secs_f32()),)}
            {data_attr("Result",     &result)}
        </dl>
    };

    let reports = LogReport::sorted(&report.log_reports)
        .iter()
        .map(|lr| render_log_report(report, lr))
        .collect::<Vec<_>>();

    let errors = report
        .read_errors
        .iter()
        .map(|(source, err)| render_log_error(source, err))
        .collect::<Vec<_>>();

    let unknowns = report
        .unknown_files
        .iter()
        .map(|(index, sources)| render_unknown(index, sources))
        .collect::<Vec<_>>();

    html! {
      <>
        {card}
        {reports}
        {errors}
        {unknowns}
      </>
    }
}

async fn get_report(path: &str) -> Result<Report, String> {
    let resp = Request::get(path)
        .send()
        .await
        .map_err(|e| format!("{}", e))?;
    let data: Vec<u8> = resp.binary().await.map_err(|e| format!("{}", e))?;
    log!(format!("Loaded report: {:?}", &data[..24]));
    logreduce_report::Report::load_bytes(&data).map_err(|e| format!("{}", e))
}

#[function_component(App)]
fn app() -> Html {
    let report: UseStateHandle<Option<Result<Report, String>>> = use_state(|| None);
    {
        let report = report.clone();
        use_effect_with_deps(
            move |_| {
                let report = report.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = get_report("report.bin").await;
                    report.set(Some(result));
                });
                || ()
            },
            (),
        );
    }

    let report_html = match report.deref() {
        Some(Ok(report)) => render_report(report),
        Some(Err(err)) => html!(<p>{err}</p>),
        None => html!(<p>{"loading..."}</p>),
    };

    let header = html! {
        <nav class={classes!("sticky", "top-0", "bg-slate-300", "z-50", "flex", "px-1", "divide-x")}>
          <div class={classes!("grow")}>{"logreduce"}</div>
          <div class={classes!("px-2", "cursor-pointer", "hover:bg-slate-400")}>
            <a href="https://github.com/logreduce/logreduce#readme" target="_black">{"documentation"}</a>
          </div>
          <div class={classes!("px-2", "rounded")}>
            <span class={classes!("font-bold")}>{"version "}</span>
            <span>{env!("CARGO_PKG_VERSION")}</span>
          </div>
        </nav>
    };

    html! {
        <>
            { header }
            { report_html }
        </>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
