mod cli;
mod config;
mod domains;
mod helpers;
mod mapping;
mod report;
mod source;
mod state;
mod target;
mod verify;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use config::Config;
use state::AppState;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let cli = Cli::parse();
    let config = Config::from_cli(cli);
    let mut state = AppState::connect(config).await?;

    if !state.config.dry_run {
        target::ensure_schema(&state.target).await?;
    }

    match state.config.command {
        Command::ResetRun => {
            target::ensure_schema(&state.target).await?;
            target::reset_run(&state.target, &state.config.run_id).await?;
        }
        Command::Verify => {
            target::ensure_schema(&state.target).await?;
            verify::run(&mut state).await?;
        }
        Command::Migrate | Command::Plan => {
            if !state.config.dry_run {
                if !state.config.resume {
                    target::reset_run(&state.target, &state.config.run_id).await?;
                }
                target::start_run(&state.target, &state.config.run_id, state.config.dry_run)
                    .await?;
            }
            let result = domains::run_migration(&mut state).await;
            if !state.config.dry_run {
                target::finish_run(
                    &state.target,
                    &state.config.run_id,
                    if result.is_ok() {
                        "completed"
                    } else {
                        "failed"
                    },
                )
                .await?;
            }
            result?;
        }
    }

    if state.config.json {
        println!("{}", serde_json::to_string_pretty(&state.report)?);
    } else {
        println!("run_id: {}", state.report.run_id);
        for domain in &state.report.domains {
            println!(
                "{} planned={} created={} updated={} skipped={} warnings={} errors={}",
                domain.name,
                domain.planned,
                domain.created,
                domain.updated,
                domain.skipped,
                domain.warnings,
                domain.errors
            );
        }
        if !state.report.warnings.is_empty() {
            println!("warnings: {}", state.report.warnings.len());
        }
        if !state.report.errors.is_empty() {
            println!("errors: {}", state.report.errors.len());
        }
    }

    Ok(())
}
