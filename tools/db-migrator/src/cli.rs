use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "db-migrator")]
#[command(
    about = "Migrates data from the current osmium Postgres schema into the Prisma-backed website schema"
)]
pub struct Cli {
    #[arg(long, env = "SOURCE_DATABASE_URL")]
    pub source_url: String,
    #[arg(long, env = "TARGET_DATABASE_URL")]
    pub target_url: String,
    #[arg(long)]
    pub run_id: Option<String>,
    #[arg(long, value_enum, default_value_t = DomainArg::All)]
    pub domain: DomainArg,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub resume: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub strict: bool,
    #[arg(long, default_value_t = false)]
    pub abort_on_warning: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum DomainArg {
    All,
    Reference,
    Users,
    Org,
    Training,
    Events,
    Feedback,
    Web,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Migrate,
    Plan,
    Verify,
    ResetRun,
}
