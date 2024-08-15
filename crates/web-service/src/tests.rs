// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

use logjuicer_model::env::EnvConfig;
use logjuicer_model::{config::DiskSizeLimit, env::Env, Model};
use logjuicer_report::{Content, Report, ZuulBuild};
use mockito::Server;
use std::sync::atomic::Ordering;
use zuul_build::zuul_manifest;

use crate::routes::ReportRequest;

fn register_build(server: &mut Server, name: &str, content: &str) -> Content {
    let mut build = ZuulBuild::sample(name);
    let path = format!("/logs/{}/artifacts/", name);
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

fn get_job_url(content: &Content) -> String {
    match content {
        Content::Zuul(b) => format!("{}job-output.txt", b.log_url.as_str()),
        _ => "unknown".into(),
    }
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

#[tokio::test]
async fn test_api_basic() {
    let mut server = mockito::Server::new_async().await;
    let env = EnvConfig::new();

    // Create builds
    let target = register_build(
        &mut server,
        "job-target",
        r#"
First good line
Oops this is an error
Second good line
"#,
    );
    let target2 = register_build(
        &mut server,
        "job-target-2",
        r#"
Oops this is an error
"#,
    );
    let baseline = register_build(
        &mut server,
        "job-success",
        r#"
First good line
Second good line
"#,
    );

    let tempdir = tempfile::tempdir().unwrap();
    let temppath = tempdir.path().to_str().unwrap();
    let workers =
        crate::worker::Workers::new(true, temppath.into(), DiskSizeLimit::default(), env).await;
    let target = get_job_url(&target);
    let baseline = get_job_url(&baseline);
    let rid = workers
        .db
        .initialize_report(&target, &baseline)
        .await
        .unwrap();
    workers.submit(
        rid,
        ReportRequest::new_request(target.clone(), baseline.clone()),
    );
    assert!(workers
        .wait(rid)
        .await
        .iter()
        .any(|msg: &std::sync::Arc<str>| msg.as_bytes() == b"Building the model"));

    // Check that the model got built
    assert_eq!(workers.db.get_models().await.unwrap().len(), 1);
    assert!(workers.current_files_size.load(Ordering::Relaxed) > 0);

    let target2 = get_job_url(&target2);
    let rid2 = workers
        .db
        .initialize_report(&target2, &baseline)
        .await
        .unwrap();
    workers.submit(rid2, ReportRequest::new_request(target, baseline));
    assert!(workers
        .wait(rid2)
        .await
        .iter()
        .any(|msg: &std::sync::Arc<str>| msg.as_bytes() == b"Loading existing model"));
    let models = workers.db.get_models().await.unwrap();
    assert_eq!(models.len(), 1);
    let model_path = crate::models::test_model_path(&workers.storage_dir, &models[0].content_id);
    assert!(model_path.exists());

    // Test old model removal
    workers.db.deprecate_models().await.unwrap();
    let env = EnvConfig::new();
    let workers =
        crate::worker::Workers::new(true, temppath.into(), DiskSizeLimit::default(), env).await;
    assert!(!model_path.exists());
    assert_eq!(workers.db.get_models().await.unwrap().len(), 0);

    // Check reclaim space
    let reports = workers.db.get_reports().await.unwrap();
    let report_size = reports[0].bytes_size.0 as usize;
    workers.db.increase_report_age().await.unwrap();
    let (amount, model_count, report_count) = workers
        .db
        .reclaim_space(
            &workers.storage_dir,
            DiskSizeLimit {
                min: report_size,
                max: report_size + 2,
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(amount > 0);
    assert_eq!(model_count, 0);
    assert_eq!(report_count, 1);
}

#[tokio::test]
async fn test_api_extra_baselines() {
    // Create builds results
    let mut server = mockito::Server::new_async().await;
    let target = register_build(
        &mut server,
        "job-target",
        r#"
First good line
Oops this is an error
Second good line
"#,
    );
    let baseline = register_build(
        &mut server,
        "job-success",
        r#"
First good line
Second good line
"#,
    );
    let extra = register_build(
        &mut server,
        "job-extra",
        r#"
Oops this is an error
"#,
    );
    let extra_url = get_job_url(&extra);
    use logjuicer_model::config::Config;
    let gl = Env::new();
    let config = Config::test_from_yaml(
        &gl,
        &format!(
            "
- match_job: job-target
  config:
    extra_baselines:
      - {extra_url}
",
        ),
    );
    let env = EnvConfig { gl, config };

    let tempdir = tempfile::tempdir().unwrap();
    let temppath = tempdir.path().to_str().unwrap();
    let workers =
        crate::worker::Workers::new(true, temppath.into(), DiskSizeLimit::default(), env).await;
    let target = get_job_url(&target);
    let baseline = get_job_url(&baseline);
    let rid = workers
        .db
        .initialize_report(&target, &baseline)
        .await
        .unwrap();
    workers.submit(rid, ReportRequest::new_request(target, baseline));
    let logs = workers.wait(rid).await;
    // dbg!(&logs);
    assert!(logs
        .iter()
        .any(|msg: &std::sync::Arc<str>| msg.as_bytes() == b"Building the model"));
    let report = Report::load(&tempdir.path().join("1.gz")).unwrap();
    // dbg!(&report);
    assert_eq!(report.total_anomaly_count, 0);
}
