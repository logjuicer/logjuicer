// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use itertools::Itertools;
use logreduce_model::{Content, Input, Model, OutputMode, Source};
use std::path::PathBuf;

mod dataset;

#[derive(Parser)]
#[clap(version, about, long_about = None)]
#[clap(disable_help_subcommand = true)]
struct Cli {
    #[clap(long, parse(from_os_str), help = "Create an html report")]
    report: Option<PathBuf>,

    #[clap(
        long,
        parse(from_os_str),
        help = "Load or save the model",
        value_name = "FILE"
    )]
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

        #[clap(long, help = "Look for baseline from the current project")]
        model_per_project: bool,

        #[clap(
            parse(from_os_str),
            help = "The zuul.executor.log_root value",
            value_name = "PATH"
        )]
        log_root: PathBuf,
    },

    #[clap(about = "Train a model")]
    Train {
        #[clap(required = true)]
        baselines: Vec<String>,
    },

    #[clap(about = "Evaluate dataset")]
    Test {
        #[clap(required = true)]
        datasets: Vec<String>,
    },

    #[clap(about = "Check a pre-built model")]
    CheckModel {
        #[clap(long, help = "Validate model age", value_name = "DAYS")]
        max_age: Option<usize>,
    },

    // Secret options to debug specific part of the process
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
}

impl Cli {
    fn run(self, progress: OutputMode) -> Result<()> {
        match self.command {
            // Discovery commands
            Commands::Path { path } => {
                process(progress, self.report, self.model, None, Input::Path(path))
            }
            Commands::Url { url } => {
                process(progress, self.report, self.model, None, Input::Url(url))
            }
            Commands::ZuulBuild {
                log_root,
                api_url,
                model_per_project,
            } => process(
                progress,
                self.report,
                self.model,
                None,
                Input::ZuulBuild(log_root, api_url, model_per_project),
            ),
            Commands::Journald { .. } => todo!(),

            // Manual commands
            Commands::Diff { src, dst } => process(
                progress,
                self.report,
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
                let model = Model::train(
                    progress,
                    baselines
                        .into_iter()
                        .map(Input::from_string)
                        .map(Content::from_input)
                        .collect::<Result<Vec<_>>>()?,
                    logreduce_model::hashing_index::new,
                )?;
                model.save(&model_path)
            }

            Commands::CheckModel { max_age } => {
                let model_path = self.model.ok_or_else(|| {
                    anyhow::anyhow!(
                        "check-model requires a path, please add a `--model FILE` argument"
                    )
                })?;
                let timestamp = Model::check(&model_path)?;
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

            Commands::Test { datasets } => dataset::test_datasets(&datasets),

            // Debug handlers
            Commands::DebugGroups { target } => debug_groups(Input::from_string(target)),
            Commands::DebugTokenizer { line } => {
                println!("{}\n", logreduce_tokenizer::process(&line));
                Ok(())
            }
            Commands::DebugIndexname { path } => {
                println!("{}", logreduce_model::IndexName::from_path(&path));
                Ok(())
            }
            Commands::DebugIterator { path } => {
                let input = Input::Path(path.clone());
                let content = Content::from_input(input)?;
                let sources = content.get_sources()?;
                match sources.first() {
                    Some(source) => {
                        let reader = match source {
                            Source::Local(_, path_buf) => Source::file_open(path_buf.as_path())?,
                            Source::Remote(prefix, url) => Source::url_open(*prefix, url)?,
                        };
                        for line in logreduce_iterator::BytesLines::new(reader, source.is_json()) {
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
        }
    }
}

fn main() -> Result<()> {
    use std::str::FromStr;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

    let logger = tracing_subscriber::Registry::default();

    let (_flush, debug) = match std::env::var("LOGREDUCE_LOG") {
        Err(_) => {
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
        Ok(level) => {
            // Tracing spans
            let logger = logger.with(
                tracing_tree::HierarchicalLayer::new(2)
                    .with_targets(true)
                    .with_bracketed_fields(true)
                    .with_filter(tracing_subscriber::filter::LevelFilter::from_str(&level)?),
            );
            let flush = if let Ok(fp) = std::env::var("LOGREDUCE_TRACE") {
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

#[tracing::instrument(level = "debug", skip(output_mode))]
fn process(
    output_mode: OutputMode,
    report: Option<PathBuf>,
    model_path: Option<PathBuf>,
    baselines: Option<Vec<Input>>,
    input: Input,
) -> Result<()> {
    // Convert user Input to target Content.
    let content = Content::from_input(input)?;

    let model = match model_path {
        Some(ref path) if path.exists() => match baselines {
            None => Model::load(path),
            Some(_) => Err(anyhow::anyhow!("Ambiguous baselines and model provided")),
        },
        _ => {
            // Lookup baselines.
            tracing::debug!("Finding baselines");
            let baselines = match baselines {
                None => content.discover_baselines(),
                Some(baselines) => baselines
                    .into_iter()
                    .map(Content::from_input)
                    .collect::<Result<Vec<_>>>(),
            }?;

            // Create the model. TODO: enable custom index.
            tracing::debug!("Building model");
            Model::train(output_mode, baselines, logreduce_model::hashing_index::new)
        }
    }?;

    match model_path {
        Some(ref path) if !path.exists() => model.save(path),
        _ => Ok(()),
    }?;

    tracing::debug!("Inspecting");
    match report {
        None => process_live(output_mode, &content, &model),
        Some(file) => {
            let report = model.report(output_mode, content)?;

            // Save raw report for debug purpose
            if std::env::var("LOGREDUCE_CACHE").is_ok() {
                let mut report_json = file.clone();
                report_json.set_extension("json");
                report.save(&report_json)?;
            }

            println!("{:?}: Writing report...", file);
            std::fs::write(
                file,
                logreduce_report::render(&report).context("Error rendering the report")?,
            )
            .context("Failed to write the report")
        }
    }
}

fn process_live(output_mode: OutputMode, content: &Content, model: &Model) -> Result<()> {
    let print_context = |pos: usize, xs: &[String]| {
        xs.iter()
            .enumerate()
            .for_each(|(idx, line)| println!("   {} | {}", pos + idx, line))
    };

    let mut progress_sep_shown = false;
    let mut total_line_count = 0;
    let mut total_anomaly_count = 0;
    for source in content.get_sources()? {
        let index_name = logreduce_model::IndexName::from_source(&source);
        match model.get_index(&index_name) {
            Some(index) => {
                let mut last_pos = None;
                let mut print_anomaly = |anomaly: logreduce_model::AnomalyContext| {
                    total_anomaly_count += 1;
                    let context_size = 1 + anomaly.before.len();
                    let starting_pos = if anomaly.anomaly.pos > context_size {
                        anomaly.anomaly.pos - context_size
                    } else {
                        0
                    };
                    if let Some(last_pos) = last_pos {
                        if last_pos != starting_pos {
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
                    output_mode,
                    &source,
                    &mut logreduce_model::unordered::KnownLines::new(),
                ) {
                    Ok(mut processor) => {
                        for anomaly in processor.by_ref() {
                            if output_mode.inlined() && !progress_sep_shown {
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
    if output_mode.inlined() && !progress_sep_shown {
        // If the last source didn't had an anomaly, then erase the current progress
        print!("\r\x1b[K");
    }
    logreduce_model::debug_or_progress(
        output_mode,
        &format!(
            "{}: Reduced from {} to {}",
            content, total_line_count, total_anomaly_count
        ),
    );
    Ok(())
}

fn debug_groups(input: Input) -> Result<()> {
    let content = Content::from_input(input)?;
    for (index_name, sources) in Content::group_sources(&[content])?
        .drain()
        .sorted_by(|x, y| Ord::cmp(&x.0, &y.0))
    {
        println!("{:?}: {:#?}", index_name, sources);
    }
    Ok(())
}
