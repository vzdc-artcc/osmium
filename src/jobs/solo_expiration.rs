use std::time::Duration;

use crate::{
    handlers::org::execute_solo_expiration,
    jobs::{Job, TickOutcome},
    repos::{audit as audit_repo, org::jobs as jobs_repo},
    state::AppState,
};

// Matches the live site's old `/api/update/solo` cron cadence (20 minutes).
const DEFAULT_INTERVAL_SECS: u64 = 1200;
const JOB_NAME: &str = "solo_expiration";

#[derive(Debug, Clone, Default)]
pub struct SoloExpirationMetrics {
    pub processed: Option<i64>,
}

struct SoloExpirationJob {
    interval_secs: u64,
}

impl Job for SoloExpirationJob {
    type Metrics = SoloExpirationMetrics;

    fn name(&self) -> &'static str {
        JOB_NAME
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        let Some(pool) = state.db.as_ref() else {
            return Err("database unavailable for solo certification expiration sweep".to_string());
        };

        // Reuses the same job_runs history and sweep+email logic as the
        // on-demand `POST /admin/jobs/solo_expiration/run` endpoint.
        let run_id = jobs_repo::create_job_run(pool, JOB_NAME)
            .await
            .map_err(|_| "failed to record solo expiration job run".to_string())?;

        let actor = audit_repo::resolve_audit_actor(pool, None, None)
            .await
            .unwrap_or(audit_repo::AuditActor { actor_id: None });

        match execute_solo_expiration(state, pool, actor).await {
            Ok(summary) => {
                let _ = jobs_repo::finish_job_run_success(
                    pool,
                    &run_id,
                    serde_json::json!({
                        "processed": summary.processed,
                        "details": summary.details,
                    }),
                )
                .await;

                Ok(TickOutcome::success(SoloExpirationMetrics {
                    processed: Some(summary.processed),
                }))
            }
            Err(error) => {
                let message = format!("solo expiration sweep failed: {error:?}");
                let _ = jobs_repo::finish_job_run_failure(pool, &run_id, &message).await;
                Err(message)
            }
        }
    }
}

pub fn start_solo_expiration_worker(state: AppState) {
    if !solo_expiration_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.solo_expiration.enabled = false;
            health.solo_expiration.last_error = None;
        }
        tracing::info!("solo expiration worker disabled");
        return;
    }

    let interval_secs = solo_expiration_interval_secs();
    if let Ok(mut health) = state.job_health.write() {
        health.solo_expiration.enabled = true;
        health.solo_expiration.last_error = None;
    }

    tracing::info!(interval_secs, "starting solo expiration worker");

    let job_health = state.job_health.clone();
    crate::jobs::spawn(
        SoloExpirationJob { interval_secs },
        state,
        job_health,
        |health| &mut health.solo_expiration,
    );
}

fn solo_expiration_enabled() -> bool {
    std::env::var("SOLO_EXPIRATION_ENABLED")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}

fn solo_expiration_interval_secs() -> u64 {
    std::env::var("SOLO_EXPIRATION_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
        .max(60)
}
