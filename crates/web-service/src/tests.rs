// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

use logjuicer_model::{env::EnvConfig, Model};
use logjuicer_report::{Content, ZuulBuild};
use mockito::Server;
use zuul_build::zuul_manifest;

fn register_build(server: &mut Server, name: &str, content: &str) -> Content {
    let mut build = ZuulBuild::sample(name);
    let path = format!("/logs/{}/", name);
    build.log_url = url::Url::parse(&server.url()).unwrap().join(&path).unwrap();
    let manifest = zuul_manifest::Manifest {
        tree: vec![zuul_manifest::Tree {
            name: "job-output.txt".into(),
            mimetype: "text/plain".into(),
            children: vec![],
        }],
    };
    server
        .mock("GET", format!("{}zuul-manifest.json", path).as_str())
        .with_body(serde_json::to_vec(&manifest).unwrap())
        .create();
    server
        .mock("GET", format!("{}job-output.txt", path).as_str())
        .with_body(content)
        .create();
    Content::Zuul(Box::new(build))
}

#[test]
fn test_model_mappend() {
    let mut server = mockito::Server::new();
    let env = EnvConfig::new();

    // Create a target build
    let target = register_build(
        &mut server,
        "job-target",
        r#"
First good line
Oops this is an error
Second good line
"#,
    );
    let target_env = env.get_target_env(&target);

    // Create a base model
    let model1 =
        Model::<logjuicer_model::FeaturesMatrix>::train::<logjuicer_model::FeaturesMatrixBuilder>(
            &target_env,
            vec![register_build(
                &mut server,
                "job-success",
                r#"
First good line
Second good line
"#,
            )],
        )
        .unwrap();

    // Check that the base model finds the anomaly
    let report = model1.report(&target_env, target.clone()).unwrap();
    assert_eq!(report.total_anomaly_count, 1);

    // Create a second model that contains the anomaly
    let model2 =
        Model::<logjuicer_model::FeaturesMatrix>::train::<logjuicer_model::FeaturesMatrixBuilder>(
            &target_env,
            vec![register_build(
                &mut server,
                "job-success-2",
                r#"
Oops this is an error
"#,
            )],
        )
        .unwrap();

    // Combine the two models and check that the anomaly is no longer reported
    let model_merged = model1.mappend(model2);
    let report = model_merged.report(&target_env, target).unwrap();
    assert_eq!(report.total_anomaly_count, 0);
}
