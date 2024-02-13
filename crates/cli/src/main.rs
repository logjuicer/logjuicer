// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

//! This module is the entrypoint of the logjuicer command line.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use itertools::Itertools;
use logjuicer_model::env::{Env, OutputMode};
use logjuicer_model::{
    content_discover_baselines, content_from_input, content_get_sources, group_sources, Content,
    FeaturesMatrix, FeaturesMatrixBuilder, Input, Model, Source,
};
use logjuicer_report::{bytes_to_mb, Report};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;
use time_humanize::{Accuracy, HumanTime, Tense};

mod dataset;

#[derive(Parser)]
#[clap(version, about, long_about = None)]
#[clap(disable_help_subcommand = true)]
struct Cli {
    #[clap(long, help = "LogJuicer configuration", value_name = "FILE")]
    config: Option<PathBuf>,

    #[clap(long, help = "Create an html report")]
    report: Option<PathBuf>,

    #[clap(
        hide = true,
        long,
        help = "Base url for web package. The version number will be added to it. Default to 'https://unpkg.com/logjuicer-web@'"
    )]
    web_package_url: Option<String>,

    #[clap(long, help = "Load or save the model", value_name = "FILE")]
    model: Option<PathBuf>,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[clap(about = "Compare targets", allow_missing_positional = true)]
    Diff { src: Vec<String>, dst: String },

    #[clap(about = "Analyze a path")]
    Path { path: String },

    #[clap(about = "Analyze a url")]
    Url { url: String },

    #[clap(about = "Analyze systemd-journal", allow_missing_positional = true)]
    Journald {
        start: Option<String>,
        range: String,
    },

    #[clap(about = "When running in CI, analyze the current build")]
    ZuulBuild {
        #[clap(long, help = "Zuul API url to fetch baselines", value_name = "URL")]
        api_url: String,

        #[clap(help = "The zuul.executor.log_root value", value_name = "PATH")]
        log_root: PathBuf,
    },

    #[clap(about = "Train a model")]
    Train {
        #[clap(required = true)]
        baselines: Vec<String>,
    },

    #[clap(about = "Evaluate datasets from the logjuicer-tests project")]
    Test {
        #[clap(required = true)]
        datasets: Vec<String>,
    },

    #[clap(about = "Check a pre-built model")]
    CheckModel {
        #[clap(long, help = "Validate model age", value_name = "DAYS")]
        max_age: Option<usize>,
    },

    #[clap(about = "Read a report")]
    ReadReport,

    // Secret options to debug specific part of the process
    #[clap(hide = true, about = "List http directory urls")]
    HttpLs { url: String },

    // Debug log files grouping
    #[clap(hide = true, about = "List source groups")]
    DebugGroups { target: String },

    // Debug tokenizer
    #[clap(hide = true, about = "Tokenize a single line")]
    DebugTokenizer { line: String },

    // Debug iterator
    #[clap(hide = true, about = "Iterate a single file")]
    DebugIterator { path: String },

    // Debug index name
    #[clap(hide = true, about = "Debug index name")]
    DebugIndexname { path: String },

    // Debug saved model
    #[clap(hide = true, about = "Debug index name")]
    DebugModel,
}

impl Cli {
    fn run(self, output: OutputMode) -> Result<()> {
        let env = Env::new_with_settings(self.config, output)?;
        match self.command {
            // Discovery commands
            Commands::Path { path } => process(
                &env,
                self.report,
                self.web_package_url,
                self.model,
                None,
                Input::Path(path),
            ),
            Commands::Url { url } => process(
                &env,
                self.report,
                self.web_package_url,
                self.model,
                None,
                Input::Url(url),
            ),
            Commands::ZuulBuild { log_root, api_url } => process(
                &env,
                self.report,
                self.web_package_url,
                self.model,
                None,
                Input::ZuulBuild(log_root, api_url),
            ),
            Commands::Journald { .. } => todo!(),

            // Manual commands
            Commands::Diff { src, dst } => process(
                &env,
                self.report,
                self.web_package_url,
                self.model,
                Some(src.into_iter().map(Input::from_string).collect()),
                Input::from_string(dst),
            ),
            Commands::Train { baselines } => {
                let model_path = self.model.ok_or_else(|| {
                    anyhow::anyhow!(
                        "A output file path is required, please add a `--model FILE` argument"
                    )
                })?;
                let model = Model::train::<FeaturesMatrixBuilder>(
                    &env,
                    baselines
                        .into_iter()
                        .map(Input::from_string)
                        .map(|x| content_from_input(&env, x))
                        .collect::<Result<Vec<_>>>()?,
                )?;
                model.save(&model_path)
            }

            Commands::CheckModel { max_age } => {
                let model_path = self.model.ok_or_else(|| {
                    anyhow::anyhow!(
                        "check-model requires a path, please add a `--model FILE` argument"
                    )
                })?;
                let timestamp = Model::<FeaturesMatrix>::check(&model_path)?;
                match max_age {
                    Some(age) => {
                        let elapsed = std::time::SystemTime::now()
                            .duration_since(timestamp)
                            .context("Duration")?;
                        if elapsed <= std::time::Duration::from_secs(3600 * 24 * age as u64) {
                            Ok(())
                        } else {
                            Err(anyhow::anyhow!("The model is too old: {:#?}", elapsed))
                        }
                    }
                    None => Ok(()),
                }?;
                println!("Good model: created_at {:#?}", timestamp);
                Ok(())
            }

            Commands::ReadReport => {
                let report_path = self.report.ok_or_else(|| {
                    anyhow::anyhow!(
                        "read-report requires a report, please add a `--report FILE` argument"
                    )
                })?;
                let report = Report::load(&report_path)?;
                print_report(report);
                Ok(())
            }

            Commands::Test { datasets } => dataset::test_datasets(&env, &datasets),

            // Debug handlers
            Commands::HttpLs { url } => {
                for url in httpdir::Crawler::new().walk(url::Url::parse(&url)?) {
                    println!("{}", url?.as_str());
                }
                Ok(())
            }
            Commands::DebugGroups { target } => debug_groups(&env, Input::from_string(target)),
            Commands::DebugTokenizer { line } => {
                println!("{}\n", logjuicer_tokenizer::process(&line));
                Ok(())
            }
            Commands::DebugIndexname { path } => {
                println!("{}", logjuicer_model::IndexName::from_path(&path));
                Ok(())
            }
            Commands::DebugIterator { path } => {
                let input = Input::Path(path.clone());
                let content = content_from_input(&env, input)?;
                let sources = content_get_sources(&content, &env)?;
                match sources.first() {
                    Some(source) => {
                        let reader = match source {
                            Source::Local(_, path_buf) => {
                                logjuicer_model::files::file_open(path_buf.as_path())?
                            }
                            Source::Remote(prefix, url) => {
                                logjuicer_model::urls::url_open(&env, *prefix, url)?
                            }
                        };
                        for line in logjuicer_iterator::BytesLines::new(reader, source.is_json()) {
                            match line {
                                Ok((bytes, nr)) => match std::str::from_utf8(&bytes) {
                                    Ok(txt) => println!("{} | {}", nr, txt),
                                    Err(e) => println!("{} | error: {}", nr, e),
                                },
                                Err(e) => println!("{}", e),
                            }
                        }
                    }
                    None => println!("{}: oops", path),
                }
                Ok(())
            }
            Commands::DebugModel => {
                let model_path = self.model.ok_or_else(|| {
                    anyhow::anyhow!(
                        "debug-model requires a path, please add a `--model FILE` argument"
                    )
                })?;
                let model = Model::load(&model_path)?;
                debug_model(model)
            }
        }
    }
}

fn main() -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

    let logger = tracing_subscriber::Registry::default();

    let (_flush, debug) = match std::env::var_os("LOGJUICER_LOG") {
        None => {
            // Default INFO stdout logger
            logger
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(false)
                        .compact()
                        .with_filter(tracing_subscriber::filter::LevelFilter::INFO),
                )
                .init();
            (None, false)
        }
        Some(_level) => {
            // Tracing spans
            let logger = logger.with(
                tracing_tree::HierarchicalLayer::new(1)
                    .with_targets(true)
                    .with_bracketed_fields(true)
                    .with_filter(tracing_subscriber::filter::EnvFilter::from_env(
                        "LOGJUICER_LOG",
                    )),
            );
            let flush = if let Ok(fp) = std::env::var("LOGJUICER_TRACE") {
                let chrome = tracing_chrome::ChromeLayerBuilder::new()
                    .file(fp)
                    .include_args(true)
                    .build();
                logger.with(chrome.0).init();
                // Return the chrome flush guard so that it is not dropped until the end
                Some(chrome.1)
            } else {
                logger.init();
                None
            };
            (flush, true)
        }
    };
    let output_mode = if debug {
        OutputMode::Debug
    } else if atty::is(atty::Stream::Stdout) {
        OutputMode::FastTerminal
    } else {
        OutputMode::Quiet
    };
    Cli::parse().run(output_mode).map_err(|e| {
        // Ensure the exception happens on a new line
        if output_mode.inlined() {
            println!();
        }
        e
    })
}

/// process is the logjuicer implementation after command line parsing.
#[tracing::instrument(level = "debug", skip(env))]
fn process(
    env: &Env,
    report: Option<PathBuf>,
    web_package_url: Option<String>,
    model_path: Option<PathBuf>,
    baselines: Option<Vec<Input>>,
    input: Input,
) -> Result<()> {
    // Convert user Input to target Content.
    let content = content_from_input(env, input)?;

    let train_model = |baselines: Option<Vec<Input>>| {
        // Lookup baselines.
        tracing::debug!("Finding baselines");
        let baselines = match baselines {
            None => content_discover_baselines(&content, env),
            Some(baselines) => baselines
                .into_iter()
                .map(|x| content_from_input(env, x))
                .collect::<Result<Vec<_>>>(),
        }?;

        // Create the model. TODO: enable custom index.
        tracing::debug!("Building model");
        Model::<FeaturesMatrix>::train::<FeaturesMatrixBuilder>(env, baselines)
    };

    let model = match model_path {
        Some(ref path) if path.exists() => match baselines {
            None => match Model::load(path) {
                Ok(model) => Ok(model),
                Err(e) => {
                    tracing::error!("Removing model becase: {:?}", e);
                    std::fs::remove_file(path)?;
                    train_model(baselines)
                }
            },
            Some(_) => Err(anyhow::anyhow!("Ambiguous baselines and model provided")),
        },
        _ => train_model(baselines),
    }?;

    match model_path {
        Some(ref path) if !path.exists() => {
            clear_progress(env.output);
            model.save(path)
        }
        _ => Ok(()),
    }?;

    tracing::debug!("Inspecting");
    match report {
        None => process_live(env, &content, &model),
        Some(file) => {
            let report = model.report(env, content)?;

            match file.extension().and_then(std::ffi::OsStr::to_str) {
                Some("bin") | Some("gz") => {
                    report
                        .save(&file)
                        .context("Failed to write the binary report")?;
                    write_html(&file, web_package_url)
                }
                .context("Failed to write the report"),
                Some("json") => serde_json::to_writer(std::fs::File::create(&file)?, &report)
                    .context("Failted to write the json report"),
                _ => Err(anyhow::anyhow!("Unknown report extension {:?}", file)),
            }?;
            tracing::info!("Wrote report {:?}", file);
            Ok(())
        }
    }
}

fn process_live(env: &Env, content: &Content, model: &Model<FeaturesMatrix>) -> Result<()> {
    let print_context = |pos: usize, xs: &[Rc<str>]| {
        xs.iter()
            .enumerate()
            .for_each(|(idx, line)| println!("   {} | {}", pos + idx, line))
    };

    let mut progress_sep_shown = false;
    let mut total_line_count = 0;
    let mut total_byte_count = 0;
    let mut total_anomaly_count = 0;
    let start_time = Instant::now();

    for source in content_get_sources(content, env)? {
        let index_name = logjuicer_model::indexname_from_source(&source);
        match model.get_index(&index_name) {
            Some(index) => {
                let mut last_pos = None;
                let mut print_anomaly = |anomaly: logjuicer_model::AnomalyContext| {
                    total_anomaly_count += 1;
                    let context_size = 1 + anomaly.before.len();
                    let starting_pos = if anomaly.anomaly.pos > context_size {
                        anomaly.anomaly.pos - context_size
                    } else {
                        0
                    };
                    if let Some(last_pos) = last_pos {
                        if last_pos < starting_pos {
                            println!("--");
                        }
                    }

                    print_context(starting_pos, &anomaly.before);
                    println!(
                        "{:02.0} {} | {}",
                        anomaly.anomaly.distance * 99.0,
                        anomaly.anomaly.pos,
                        anomaly.anomaly.line
                    );
                    print_context(anomaly.anomaly.pos, &anomaly.after);

                    last_pos = Some(anomaly.anomaly.pos + anomaly.after.len());
                };
                progress_sep_shown = false;
                match index.get_processor(
                    env,
                    &source,
                    &mut Some(logjuicer_model::unordered::KnownLines::new()),
                ) {
                    Ok(mut processor) => {
                        for anomaly in processor.by_ref() {
                            if env.output.inlined() && !progress_sep_shown {
                                // Show a progress separator for the first anomaly.
                                println!();
                                progress_sep_shown = true;
                            }
                            match anomaly {
                                Ok(anomaly) => print_anomaly(anomaly),
                                Err(err) => {
                                    println!("Could not read {}: {}", &source, err);
                                    break;
                                }
                            }
                        }
                        total_line_count += processor.line_count;
                        total_byte_count += processor.byte_count;
                    }
                    Err(err) => {
                        println!("Could not read {}: {}", &source, err);
                        break;
                    }
                }
            }
            None => {
                progress_sep_shown = true;
                println!(" -> No baselines for {}", source)
            }
        }
    }
    if !progress_sep_shown {
        // If the last source didn't had an anomaly, then erase the current progress
        clear_progress(env.output);
    }
    let process_time = start_time.elapsed();
    let total_mb_count = (total_byte_count as f64) / (1024.0 * 1024.0);
    let speed: f64 = total_mb_count / process_time.as_secs_f64();
    env.debug_or_progress(&format!(
        "Completed {}: Reduced from {} to {} {} at {:.2} MB/s\n",
        content,
        total_line_count,
        total_anomaly_count,
        HumanTime::from(process_time),
        speed,
    ));
    Ok(())
}

fn debug_groups(env: &Env, input: Input) -> Result<()> {
    let content = content_from_input(env, input)?;
    for (index_name, sources) in group_sources(env, &[content])?
        .drain()
        .sorted_by(|x, y| Ord::cmp(&x.0, &y.0))
    {
        println!("{}:", index_name);
        sources.iter().for_each(|source| {
            println!("  {}", source);
        });
    }
    Ok(())
}

fn print_created(time: std::time::SystemTime) {
    println!(
        "created: {}",
        HumanTime::from(time).to_text_en(Accuracy::Rough, Tense::Past)
    )
}

fn debug_model(model: Model<FeaturesMatrix>) -> Result<()> {
    print_created(model.created_at);
    println!("baselines:");
    model.baselines.iter().for_each(|content| {
        println!("  {}", content);
    });
    println!("indexes:");
    model
        .indexes
        .iter()
        .sorted_by(|x, y| Ord::cmp(&x.0, &y.0))
        .for_each(|(indexname, index)| {
            println!("- {}:", indexname);
            println!(
                "  info: {} msec, {} lines, {:.2} MB",
                index.train_time.as_millis(),
                index.line_count,
                bytes_to_mb(index.byte_count)
            );
            println!("  samples: {}", index.samples_count());
            index.sources.iter().for_each(|source| {
                println!("  from: {}", source);
            })
        });
    Ok(())
}

fn print_report(report: Report) {
    print_created(report.created_at);
    println!("target: {}", report.target);
    println!("baselines:");
    report.baselines.iter().for_each(|content| {
        println!("  {}", content);
    });
    println!("logs:");
    report.log_reports.iter().for_each(|log_report| {
        println!("- {}", log_report.source);
        println!(
            "  info: {}/{} lines, {:.2} MB",
            log_report.anomalies.len(),
            log_report.line_count,
            bytes_to_mb(log_report.byte_count)
        );
        log_report.anomalies.iter().for_each(|anomaly_context| {
            println!(
                "  {}: {}",
                anomaly_context.anomaly.pos, anomaly_context.anomaly.line
            );
        })
    })
}

fn clear_progress(output_mode: OutputMode) {
    if output_mode.inlined() {
        print!("\r\x1b[K");
    }
}

fn write_html(report: &std::path::Path, web_package_url: Option<String>) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let base_assets_url = match web_package_url {
        Some(url) => format!("{url}{version}"),
        None => format!("https://unpkg.com/logjuicer-web@{version}"),
    };
    let assets_url = format!("{base_assets_url}/logjuicer-web");
    let report_file_name = report
        .file_name()
        .and_then(|os| os.to_str())
        .ok_or(anyhow::anyhow!("Invalid path {:?}", report))?;
    let report_script = format!("window.report = '{report_file_name}';");

    let index = format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8">
<title>LogJuicer</title>
<link rel="stylesheet" href="{assets_url}.css">
<link rel="icon" href="{base_assets_url}/LogJuicer.svg" />
<link rel="preload" href="{assets_url}.wasm" as="fetch" type="application/wasm" crossorigin="">
<link rel="modulepreload" href="{assets_url}.js"></head>
<body><script type="module">{report_script}import init from '{assets_url}.js';init();</script></body></html>"#
    );
    std::fs::write(report.with_extension("html"), index).context("Failed to write the html file")
}
