// Copyright (C) 2022 Red Hat
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::{Parser, Subcommand};
use logreduce_model::{Content, Input, Model};

#[derive(Parser)]
#[clap(version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(long)]
    report: Option<String>,

    #[clap(subcommand)]
    command: Commands,

    // Secret options to debug source groups
    #[clap(long, hide = true)]
    debug_groups: bool,
}

#[derive(Subcommand)]
enum Commands {
    Diff { src: String, dst: String },
    Path { path: String },
}

impl Commands {
    fn get_input(&self) -> (Option<Input>, Input) {
        match self {
            Commands::Diff { src, dst } => (
                Some(Input::Path(src.to_string())),
                Input::Path(dst.to_string()),
            ),
            Commands::Path { path } => (None, Input::Path(path.to_string())),
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
    let args = Cli::parse();
    let (baseline, input) = args.command.get_input();
    if args.debug_groups {
        debug_groups(input)
    } else {
        process(args.report, baseline, input)
    }
}

#[tracing::instrument]
fn process(report: Option<String>, baseline: Option<Input>, input: Input) -> Result<()> {
    // Convert user Input to target Content.
    tracing::debug!("Discovering content type");
    let content = Content::from_input(input)?;

    // Lookup baselines.
    tracing::debug!("Discovering baselines");
    let baselines = match baseline {
        None => content.discover_baselines()?,
        Some(baseline) => vec![Content::from_input(baseline)?],
    };

    // Create the model. TODO: enable custom index.
    tracing::debug!("Building model");
    let model = Model::train(baselines, logreduce_model::hashing_index::new)?;

    tracing::debug!("Inspecting");
    match report {
        None => process_live(&content, &model),
        Some(file) => {
            let report = model.report(&content)?;
            println!("{}: Writing report {:?}", file, report);
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

    for source in content.get_sources() {
        let source = source?;
        let index = model.get_index(&source).expect("Missing baselines");
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

    Ok(())
}

fn debug_groups(input: Input) -> Result<()> {
    let content = Content::from_input(input)?;
    for (index_name, sources) in Content::group_sources(&[content])?.drain() {
        println!("{:?}: {:#?}", index_name, sources);
    }
    Ok(())
}
