use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[derive(Serialize)]
pub struct HealthBody {
    status: &'static str,
}

#[derive(Serialize)]
pub struct ReadyBody {
    status: &'static str,
    database: &'static str,
    docs: &'static str,
    jobs: JobsHealthBody,
}

#[derive(Serialize)]
pub struct JobsHealthBody {
    stats_sync: StatsSyncHealthBody,
    roster_sync: RosterSyncHealthBody,
    email_worker: EmailWorkerHealthBody,
}

#[derive(Serialize)]
pub struct StatsSyncHealthBody {
    enabled: bool,
    stale_after_seconds: i64,
    live: StatsSyncEnvironmentHealthBody,
    sweatbox1: StatsSyncEnvironmentHealthBody,
    sweatbox2: StatsSyncEnvironmentHealthBody,
}

#[derive(Serialize)]
pub struct StatsSyncEnvironmentHealthBody {
    stale: bool,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_started_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_finished_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_success_at: Option<DateTime<Utc>>,
    last_result_ok: Option<bool>,
    last_error: Option<String>,
    processed: Option<usize>,
    online: Option<usize>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct RosterSyncHealthBody {
    enabled: bool,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_started_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_finished_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_success_at: Option<DateTime<Utc>>,
    last_result_ok: Option<bool>,
    last_error: Option<String>,
    processed: Option<usize>,
    matched: Option<usize>,
    updated: Option<usize>,
    demoted: Option<usize>,
    skipped: Option<usize>,
}

#[derive(Serialize)]
pub struct EmailWorkerHealthBody {
    enabled: bool,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_started_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_finished_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    last_success_at: Option<DateTime<Utc>>,
    last_result_ok: Option<bool>,
    last_error: Option<String>,
    pending_count: Option<i64>,
}

pub async fn health(time: ResponseTimeContext) -> ApiJson<HealthBody> {
    ApiJson::new(HealthBody { status: "ok" }, time)
}

pub async fn ready(State(state): State<AppState>, time: ResponseTimeContext) -> ApiJson<ReadyBody> {
    let database_ready = if let Some(pool) = state.db {
        sqlx::query_scalar::<_, i32>("select 1")
            .fetch_one(&pool)
            .await
            .is_ok()
    } else {
        false
    };

    let stats_snapshot = state
        .job_health
        .read()
        .ok()
        .map(|health| health.stats_sync.clone());

    let stats_sync = if let Some(stats) = stats_snapshot {
        let stale_after_seconds = std::env::var("STATS_SYNC_STALE_SECS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(300)
            .max(30);

        StatsSyncHealthBody {
            enabled: stats.enabled,
            stale_after_seconds,
            live: environment_health_body(&stats.live, stale_after_seconds),
            sweatbox1: environment_health_body(&stats.sweatbox1, stale_after_seconds),
            sweatbox2: environment_health_body(&stats.sweatbox2, stale_after_seconds),
        }
    } else {
        StatsSyncHealthBody {
            enabled: false,
            stale_after_seconds: 300,
            live: poisoned_health_body(),
            sweatbox1: poisoned_health_body(),
            sweatbox2: poisoned_health_body(),
        }
    };

    let live_stale = stats_sync.enabled && stats_sync.live.stale;
    let roster_sync = state
        .job_health
        .read()
        .ok()
        .map(|health| roster_health_body(&health.roster_sync))
        .unwrap_or_else(poisoned_roster_health_body);
    let email_worker = state
        .email_health
        .read()
        .ok()
        .map(|health| email_health_body(&health.worker))
        .unwrap_or_else(poisoned_email_health_body);

    ApiJson::new(
        ReadyBody {
            status: if database_ready && !live_stale {
                "ready"
            } else {
                "degraded"
            },
            database: if database_ready { "ready" } else { "degraded" },
            docs: "ready",
            jobs: JobsHealthBody {
                stats_sync,
                roster_sync,
                email_worker,
            },
        },
        time,
    )
}

fn environment_health_body(
    stats: &crate::state::StatsSyncEnvironmentHealth,
    stale_after_seconds: i64,
) -> StatsSyncEnvironmentHealthBody {
    let stale = stats
        .last_success_at
        .map(|last| Utc::now() - last > chrono::Duration::seconds(stale_after_seconds))
        .unwrap_or(true);

    StatsSyncEnvironmentHealthBody {
        stale,
        last_started_at: stats.last_started_at,
        last_finished_at: stats.last_finished_at,
        last_success_at: stats.last_success_at,
        last_result_ok: stats.last_result_ok,
        last_error: stats.last_error.clone(),
        processed: stats.metrics.processed,
        online: stats.metrics.online,
        source_updated_at: stats.metrics.source_updated_at,
    }
}

fn poisoned_health_body() -> StatsSyncEnvironmentHealthBody {
    StatsSyncEnvironmentHealthBody {
        stale: true,
        last_started_at: None,
        last_finished_at: None,
        last_success_at: None,
        last_result_ok: None,
        last_error: Some("job health lock poisoned".to_string()),
        processed: None,
        online: None,
        source_updated_at: None,
    }
}

fn roster_health_body(stats: &crate::state::RosterSyncHealth) -> RosterSyncHealthBody {
    RosterSyncHealthBody {
        enabled: stats.enabled,
        last_started_at: stats.last_started_at,
        last_finished_at: stats.last_finished_at,
        last_success_at: stats.last_success_at,
        last_result_ok: stats.last_result_ok,
        last_error: stats.last_error.clone(),
        processed: stats.metrics.processed,
        matched: stats.metrics.matched,
        updated: stats.metrics.updated,
        demoted: stats.metrics.demoted,
        skipped: stats.metrics.skipped,
    }
}

fn poisoned_roster_health_body() -> RosterSyncHealthBody {
    RosterSyncHealthBody {
        enabled: false,
        last_started_at: None,
        last_finished_at: None,
        last_success_at: None,
        last_result_ok: None,
        last_error: Some("job health lock poisoned".to_string()),
        processed: None,
        matched: None,
        updated: None,
        demoted: None,
        skipped: None,
    }
}

fn email_health_body(stats: &crate::email::EmailWorkerHealth) -> EmailWorkerHealthBody {
    EmailWorkerHealthBody {
        enabled: stats.enabled,
        last_started_at: stats.last_started_at,
        last_finished_at: stats.last_finished_at,
        last_success_at: stats.last_success_at,
        last_result_ok: stats.last_result_ok,
        last_error: stats.last_error.clone(),
        pending_count: stats.metrics.pending_count,
    }
}

fn poisoned_email_health_body() -> EmailWorkerHealthBody {
    EmailWorkerHealthBody {
        enabled: false,
        last_started_at: None,
        last_finished_at: None,
        last_success_at: None,
        last_result_ok: None,
        last_error: Some("email health lock poisoned".to_string()),
        pending_count: None,
    }
}
