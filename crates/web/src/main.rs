use gloo_console::log;
use gloo_net::http::Request;
use std::ops::Deref;
use yew::prelude::*;

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

    let report_str = match report.deref() {
        Some(report) => format!("{}", report.target),
        None => "loading...".to_string(),
    };

    html! {
        <div>
            {"Logreduce web interface"}
            <p>{ report_str }</p>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
