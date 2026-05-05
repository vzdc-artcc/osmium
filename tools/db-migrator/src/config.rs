use chrono::Utc;
use uuid::Uuid;

use crate::cli::{Cli, Command, DomainArg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Reference,
    Users,
    Org,
    Training,
    Feedback,
    Events,
    Web,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub source_url: String,
    pub target_url: String,
    pub run_id: String,
    pub domains: Vec<Domain>,
    pub dry_run: bool,
    pub resume: bool,
    pub strict: bool,
    pub abort_on_warning: bool,
    pub json: bool,
    pub command: Command,
}

impl Config {
    pub fn from_cli(cli: Cli) -> Self {
        let run_id = cli.run_id.unwrap_or_else(|| {
            format!(
                "run-{}-{}",
                Utc::now().format("%Y%m%d%H%M%S"),
                Uuid::new_v4().simple()
            )
        });

        let domains = match cli.domain {
            DomainArg::All => vec![
                Domain::Reference,
                Domain::Users,
                Domain::Org,
                Domain::Training,
                Domain::Feedback,
                Domain::Events,
            ],
            DomainArg::Reference => vec![Domain::Reference],
            DomainArg::Users => vec![Domain::Users],
            DomainArg::Org => vec![Domain::Org],
            DomainArg::Training => vec![Domain::Training],
            DomainArg::Feedback => vec![Domain::Feedback],
            DomainArg::Events => vec![Domain::Events],
            DomainArg::Web => vec![Domain::Web],
        };

        Self {
            source_url: cli.source_url,
            target_url: cli.target_url,
            run_id,
            domains,
            dry_run: cli.dry_run || matches!(cli.command, Command::Plan),
            resume: cli.resume,
            strict: cli.strict,
            abort_on_warning: cli.abort_on_warning,
            json: cli.json,
            command: cli.command,
        }
    }
}
