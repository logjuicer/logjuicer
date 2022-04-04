// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::{Parser, Subcommand};
use logreduce_model::{Content, Input, Model};
use std::path::PathBuf;

#[derive(Parser)]
#[clap(version, about, long_about = None)]
#[clap(disable_help_subcommand = true)]
struct Cli {
    #[clap(long, parse(from_os_str), help = "Create an html report")]
    report: Option<PathBuf>,

    #[clap(long, parse(from_os_str), help = "Load or save the model")]
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
    CurrentBuild,

    #[clap(about = "Train a model")]
    Train {
        #[clap(required = true)]
        baselines: Vec<String>,
    },

    // Secret options to debug specific part of the process
    #[clap(hide = true, about = "List source groups")]
    DebugGroups { target: String },
}

impl Cli {
    fn run(self) -> Result<()> {
        match self.command {
            // Discovery commands
            Commands::Path { path } => process(self.report, self.model, None, Input::Path(path)),
            Commands::Url { url } => process(self.report, self.model, None, Input::Url(url)),
            Commands::Journald { .. } => todo!(),
            Commands::CurrentBuild => todo!(),

            // Manual commands
            Commands::Diff { src, dst } => process(
                self.report,
                self.model,
                Some(src.into_iter().map(Input::from_string).collect()),
                Input::from_string(dst),
            ),
            Commands::Train { baselines } => {
                let model_path = self
                    .model
                    .ok_or_else(|| anyhow::anyhow!("--model is required"))?;
                let model = Model::train(
                    baselines
                        .into_iter()
                        .map(Input::from_string)
                        .map(Content::from_input)
                        .collect::<Result<Vec<_>>>()?,
                    logreduce_model::hashing_index::new,
                )?;
                model.save(&model_path)
            }

            // Debug handlers
            Commands::DebugGroups { target } => debug_groups(Input::from_string(target)),
        }
    }
}

fn main() -> Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
    tracing_subscriber::Registry::default()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(
            tracing_tree::HierarchicalLayer::new(2)
                .with_targets(true)
                .with_bracketed_fields(true),
        )
        .init();
    Cli::parse().run()
}

#[tracing::instrument]
fn process(
    report: Option<PathBuf>,
    model_path: Option<PathBuf>,
    baselines: Option<Vec<Input>>,
    input: Input,
) -> Result<()> {
    // Convert user Input to target Content.
    tracing::debug!("Discovering content type");
    let content = Content::from_input(input)?;

    let model = match model_path {
        Some(ref path) if path.exists() => match baselines {
            None => Model::load(path),
            Some(_) => Err(anyhow::anyhow!("Ambiguous baselines and model provided")),
        },
        _ => {
            // Lookup baselines.
            let baselines = match baselines {
                None => {
                    tracing::debug!("Discovering baselines");
                    content.discover_baselines()
                }
                Some(baselines) => baselines
                    .into_iter()
                    .map(Content::from_input)
                    .collect::<Result<Vec<_>>>(),
            }?;

            // Create the model. TODO: enable custom index.
            tracing::debug!("Building model");
            Model::train(baselines, logreduce_model::hashing_index::new)
        }
    }?;

    match model_path {
        Some(ref path) if !path.exists() => model.save(path),
        _ => Ok(()),
    }?;

    tracing::debug!("Inspecting");
    match report {
        None => process_live(&content, &model),
        Some(file) => {
            let report = model.report(&content)?;
            println!("{:?}: Writing report {:?}", file, report);
            Ok(())
        }
    }
}

fn process_live(content: &Content, model: &Model) -> Result<()> {
    let print_context = |pos: usize, xs: &[String]| {
        xs.iter()
            .enumerate()
            .for_each(|(idx, line)| println!("   {} | {}", pos + idx, line))
    };

    for source in content.get_sources()? {
        match model.get_index(&source) {
            Some(index) => {
                let mut last_pos = None;
                for anomaly in index.inspect(source) {
                    let anomaly = anomaly?;
                    let starting_pos = anomaly.anomaly.pos - 1 - anomaly.before.len();
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
                }
            }
            None => println!("No baselines for {:?}", source),
        }
    }

    Ok(())
}

fn debug_groups(input: Input) -> Result<()> {
    let content = Content::from_input(input)?;
    for (index_name, sources) in Content::group_sources(&[content])?.drain() {
        println!("{:?}: {:#?}", index_name, sources);
    }
    Ok(())
}
