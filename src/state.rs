use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};

#[derive(Clone, Default)]
pub struct JobHealth {
    pub stats_sync: StatsSyncHealth,
}

#[derive(Clone, Default)]
pub struct StatsSyncHealth {
    pub enabled: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_result_ok: Option<bool>,
    pub processed: Option<usize>,
    pub online: Option<usize>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Option<PgPool>,
    pub job_health: Arc<RwLock<JobHealth>>,
}

impl AppState {
    pub async fn from_env() -> Result<Self, sqlx::Error> {
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(&database_url)
                .await?;
            return Ok(Self {
                db: Some(pool),
                job_health: Arc::new(RwLock::new(JobHealth::default())),
            });
        }

        Ok(Self {
            db: None,
            job_health: Arc::new(RwLock::new(JobHealth::default())),
        })
    }

    pub fn without_db() -> Self {
        Self {
            db: None,
            job_health: Arc::new(RwLock::new(JobHealth::default())),
        }
    }
}
