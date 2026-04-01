use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::state::AppState;

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
}

#[derive(Serialize)]
pub struct StatsSyncHealthBody {
    enabled: bool,
    stale: bool,
    stale_after_seconds: i64,
    last_started_at: Option<DateTime<Utc>>,
    last_finished_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    last_result_ok: Option<bool>,
    last_error: Option<String>,
    processed: Option<usize>,
    online: Option<usize>,
}

pub async fn health() -> Json<HealthBody> {
    Json(HealthBody { status: "ok" })
}

pub async fn ready(State(state): State<AppState>) -> Json<ReadyBody> {
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
        let stale = stats.enabled
            && stats
                .last_success_at
                .map(|last| Utc::now() - last > chrono::Duration::seconds(stale_after_seconds))
                .unwrap_or(true);

        StatsSyncHealthBody {
            enabled: stats.enabled,
            stale,
            stale_after_seconds,
            last_started_at: stats.last_started_at,
            last_finished_at: stats.last_finished_at,
            last_success_at: stats.last_success_at,
            last_result_ok: stats.last_result_ok,
            last_error: stats.last_error,
            processed: stats.processed,
            online: stats.online,
        }
    } else {
        StatsSyncHealthBody {
            enabled: false,
            stale: true,
            stale_after_seconds: 300,
            last_started_at: None,
            last_finished_at: None,
            last_success_at: None,
            last_result_ok: None,
            last_error: Some("job health lock poisoned".to_string()),
            processed: None,
            online: None,
        }
    };

    Json(ReadyBody {
        status: if database_ready && !stats_sync.stale {
            "ready"
        } else {
            "degraded"
        },
        database: if database_ready { "ready" } else { "degraded" },
        docs: "ready",
        jobs: JobsHealthBody { stats_sync },
    })
}
