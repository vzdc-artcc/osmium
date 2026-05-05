use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::{
    config::{AuthMode, CommandKind, Config, RawConfig, parse_csv},
    personas::Persona,
};

#[derive(Debug, Parser)]
#[command(name = "api-load-tester")]
#[command(about = "Standalone Osmium API timing, load, and scenario runner")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub shared: SharedArgs,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Run(SharedArgs),
    Sweep(SharedArgs),
    Load(SharedArgs),
    Scenario(SharedArgs),
    Discover(SharedArgs),
}

#[derive(Debug, Clone, Args, Default)]
pub struct SharedArgs {
    #[arg(long)]
    pub base_url: Option<String>,

    #[arg(long)]
    pub report_dir: Option<PathBuf>,

    #[arg(long)]
    pub seed_dev_data: bool,

    #[arg(long)]
    pub auth_mode: Option<String>,

    #[arg(long)]
    pub include_tags: Option<String>,

    #[arg(long)]
    pub exclude_tags: Option<String>,

    #[arg(long)]
    pub include_methods: Option<String>,

    #[arg(long)]
    pub exclude_path_regex: Option<String>,

    #[arg(long = "scenario")]
    pub scenarios: Vec<String>,

    #[arg(long)]
    pub personas: Option<String>,

    #[arg(long)]
    pub burst_requests: Option<usize>,

    #[arg(long)]
    pub burst_concurrency: Option<usize>,

    #[arg(long)]
    pub route_timeout_ms: Option<u64>,

    #[arg(long)]
    pub fail_fast: bool,

    #[arg(long)]
    pub json_out: Option<PathBuf>,

    #[arg(long)]
    pub unsafe_mutations: bool,
}

impl Cli {
    pub fn into_config(self) -> Result<Config> {
        let (command, args) = match self.command {
            Some(Commands::Run(args)) => (CommandKind::Run, merge_args(self.shared, args)),
            Some(Commands::Sweep(args)) => (CommandKind::Sweep, merge_args(self.shared, args)),
            Some(Commands::Load(args)) => (CommandKind::Load, merge_args(self.shared, args)),
            Some(Commands::Scenario(args)) => {
                (CommandKind::Scenario, merge_args(self.shared, args))
            }
            Some(Commands::Discover(args)) => {
                (CommandKind::Discover, merge_args(self.shared, args))
            }
            None => (CommandKind::Run, self.shared),
        };

        Config::from_raw(raw_from_args(command, args)?)
    }
}

fn merge_args(base: SharedArgs, sub: SharedArgs) -> SharedArgs {
    SharedArgs {
        base_url: sub.base_url.or(base.base_url),
        report_dir: sub.report_dir.or(base.report_dir),
        seed_dev_data: base.seed_dev_data || sub.seed_dev_data,
        auth_mode: sub.auth_mode.or(base.auth_mode),
        include_tags: sub.include_tags.or(base.include_tags),
        exclude_tags: sub.exclude_tags.or(base.exclude_tags),
        include_methods: sub.include_methods.or(base.include_methods),
        exclude_path_regex: sub.exclude_path_regex.or(base.exclude_path_regex),
        scenarios: if sub.scenarios.is_empty() {
            base.scenarios
        } else {
            sub.scenarios
        },
        personas: sub.personas.or(base.personas),
        burst_requests: sub.burst_requests.or(base.burst_requests),
        burst_concurrency: sub.burst_concurrency.or(base.burst_concurrency),
        route_timeout_ms: sub.route_timeout_ms.or(base.route_timeout_ms),
        fail_fast: base.fail_fast || sub.fail_fast,
        json_out: sub.json_out.or(base.json_out),
        unsafe_mutations: base.unsafe_mutations || sub.unsafe_mutations,
    }
}

fn raw_from_args(command: CommandKind, args: SharedArgs) -> Result<RawConfig> {
    let auth_mode = args.auth_mode.as_deref().map(AuthMode::parse).transpose()?;
    let personas = args
        .personas
        .as_deref()
        .map(parse_csv)
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.parse::<Persona>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(anyhow::Error::msg)?;

    Ok(RawConfig {
        command: Some(command),
        base_url: args.base_url,
        report_dir: args.report_dir,
        seed_dev_data: args.seed_dev_data.then_some(true),
        auth_mode,
        include_tags: args.include_tags.as_deref().map(parse_csv),
        exclude_tags: args.exclude_tags.as_deref().map(parse_csv),
        include_methods: args.include_methods.as_deref().map(parse_csv),
        exclude_path_regex: args.exclude_path_regex,
        scenarios: (!args.scenarios.is_empty()).then_some(args.scenarios),
        personas: (!personas.is_empty()).then_some(personas),
        burst_requests: args.burst_requests,
        burst_concurrency: args.burst_concurrency,
        route_timeout_ms: args.route_timeout_ms,
        fail_fast: args.fail_fast.then_some(true),
        json_out: args.json_out,
        unsafe_mutations: args.unsafe_mutations.then_some(true),
    })
}
