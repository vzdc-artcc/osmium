use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::{config::Config, report::MigrationReport};

pub struct AppState {
    pub config: Config,
    pub source: PgPool,
    pub target: PgPool,
    pub report: MigrationReport,
}

impl AppState {
    pub async fn connect(config: Config) -> Result<Self> {
        let source = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.source_url)
            .await
            .context("failed to connect to source database")?;
        let target = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.target_url)
            .await
            .context("failed to connect to target database")?;
        let report = MigrationReport::new(config.run_id.clone());
        Ok(Self {
            config,
            source,
            target,
            report,
        })
    }
}
