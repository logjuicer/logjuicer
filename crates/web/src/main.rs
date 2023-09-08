use gloo_net::http::Request;
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
                    let fetched_report: String = Request::get("/data.json")
                        .send()
                        .await
                        .unwrap()
                        .json()
                        .await
                        .expect("string?");
                    report.set(Some(fetched_report));
                });
                || ()
            },
            (),
        );
    }

    let counter = use_state(|| 0);
    let onclick = {
        let counter = counter.clone();
        move |_| {
            let value = *counter + 1;
            counter.set(value);
        }
    };

    let report_str = match (*report).clone() {
        Some(report) => report,
        None => "loading...".to_string(),
    };

    html! {
        <div>
            {"Hello !"}
            <button {onclick}>{ "+1" }</button>
            <p>{ *counter }</p>
            <p>{ report_str }</p>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
