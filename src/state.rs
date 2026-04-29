use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::sync::broadcast;

use crate::jobs::stats_sync::ControllerLifecycleEvent;

#[derive(Clone, Default)]
pub struct JobHealth {
    pub stats_sync: StatsSyncHealth,
    pub roster_sync: RosterSyncHealth,
}

#[derive(Clone, Default)]
pub struct StatsSyncHealth {
    pub enabled: bool,
    pub live: StatsSyncEnvironmentHealth,
    pub sweatbox1: StatsSyncEnvironmentHealth,
    pub sweatbox2: StatsSyncEnvironmentHealth,
}

#[derive(Clone, Default)]
pub struct StatsSyncEnvironmentHealth {
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_result_ok: Option<bool>,
    pub processed: Option<usize>,
    pub online: Option<usize>,
    pub source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Default)]
pub struct RosterSyncHealth {
    pub enabled: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_result_ok: Option<bool>,
    pub last_error: Option<String>,
    pub processed: Option<usize>,
    pub matched: Option<usize>,
    pub updated: Option<usize>,
    pub demoted: Option<usize>,
    pub skipped: Option<usize>,
}

impl StatsSyncHealth {
    pub fn environment_mut(&mut self, environment: &str) -> &mut StatsSyncEnvironmentHealth {
        match environment {
            "live" => &mut self.live,
            "sweatbox1" => &mut self.sweatbox1,
            "sweatbox2" => &mut self.sweatbox2,
            _ => &mut self.live,
        }
    }

    pub fn environment(&self, environment: &str) -> &StatsSyncEnvironmentHealth {
        match environment {
            "live" => &self.live,
            "sweatbox1" => &self.sweatbox1,
            "sweatbox2" => &self.sweatbox2,
            _ => &self.live,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Option<PgPool>,
    pub job_health: Arc<RwLock<JobHealth>>,
    pub controller_events: broadcast::Sender<ControllerLifecycleEvent>,
}

impl AppState {
    pub async fn from_env() -> Result<Self, sqlx::Error> {
        let (controller_events, _) = broadcast::channel(1024);
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(&database_url)
                .await?;
            return Ok(Self {
                db: Some(pool),
                job_health: Arc::new(RwLock::new(JobHealth::default())),
                controller_events,
            });
        }

        Ok(Self {
            db: None,
            job_health: Arc::new(RwLock::new(JobHealth::default())),
            controller_events,
        })
    }

    pub fn without_db() -> Self {
        let (controller_events, _) = broadcast::channel(1024);
        Self {
            db: None,
            job_health: Arc::new(RwLock::new(JobHealth::default())),
            controller_events,
        }
    }
}
