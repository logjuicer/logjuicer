use gloo_console::log;
use gloo_net::http::Request;
use std::ops::Deref;
use yew::prelude::*;

fn render_report(report: &logreduce_report::Report) -> Html {
    html! {
      <>
           <p>{ format!("{}", report.target) }</p>
           <div class={classes!("m-2")}>{format!("Anomaly count: {}", report.total_anomaly_count)}</div>
      </>
    }
}

#[function_component(App)]
fn app() -> Html {
    let report = use_state(|| None);
    {
        let report = report.clone();
        use_effect_with_deps(
            move |_| {
                let report = report.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let data: Vec<u8> = Request::get("/report.bin")
                        .send()
                        .await
                        .unwrap()
                        .binary()
                        .await
                        .expect("binary");
                    log!(format!("Loaded report: {:?}", &data[..24]));
                    let fetched_report =
                        logreduce_report::Report::load_bytes(&data).expect("report");
                    report.set(Some(fetched_report));
                });
                || ()
            },
            (),
        );
    }

    let report_html = match report.deref() {
        Some(report) => render_report(report),
        None => html!(<p>{"loading..."}</p>),
    };

    html! {
        <div>
            {"Logreduce web interface"}
            <p>{ report_html }</p>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
