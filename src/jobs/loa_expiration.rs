use std::time::Duration;

use crate::{
    handlers::org::execute_loa_expiration,
    jobs::{Job, TickOutcome},
    repos::{audit as audit_repo, org::jobs as jobs_repo},
    state::AppState,
};

// Matches the live site's old `/api/update/loa` cron cadence (20 minutes),
// unlike event_lifecycle's tighter 300s default — LOA expiration isn't
// time-sensitive in the same way event lock/archive transitions are.
const DEFAULT_INTERVAL_SECS: u64 = 1200;
const JOB_NAME: &str = "loa_expiration";

#[derive(Debug, Clone, Default)]
pub struct LoaExpirationMetrics {
    pub processed: Option<i64>,
}

struct LoaExpirationJob {
    interval_secs: u64,
}

impl Job for LoaExpirationJob {
    type Metrics = LoaExpirationMetrics;

    fn name(&self) -> &'static str {
        JOB_NAME
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        let Some(pool) = state.db.as_ref() else {
            return Err("database unavailable for LOA expiration sweep".to_string());
        };

        // Reuses the same job_runs history the on-demand
        // `POST /admin/jobs/loa_expiration/run` endpoint writes to, and the
        // same sweep+email logic, so automatic and manually-triggered runs
        // behave identically and show up together in the admin jobs UI.
        let run_id = jobs_repo::create_job_run(pool, JOB_NAME)
            .await
            .map_err(|_| "failed to record LOA expiration job run".to_string())?;

        // No logged-in user for an automatic sweep — resolves to a system
        // actor (`actor_id: None`), same pattern used for other unattended
        // job runs.
        let actor = audit_repo::resolve_audit_actor(pool, None, None)
            .await
            .unwrap_or(audit_repo::AuditActor { actor_id: None });

        match execute_loa_expiration(state, pool, actor).await {
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

                Ok(TickOutcome::success(LoaExpirationMetrics {
                    processed: Some(summary.processed),
                }))
            }
            Err(error) => {
                let message = format!("LOA expiration sweep failed: {error:?}");
                let _ = jobs_repo::finish_job_run_failure(pool, &run_id, &message).await;
                Err(message)
            }
        }
    }
}

pub fn start_loa_expiration_worker(state: AppState) {
    if !loa_expiration_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.loa_expiration.enabled = false;
            health.loa_expiration.last_error = None;
        }
        tracing::info!("LOA expiration worker disabled");
        return;
    }

    let interval_secs = loa_expiration_interval_secs();
    if let Ok(mut health) = state.job_health.write() {
        health.loa_expiration.enabled = true;
        health.loa_expiration.last_error = None;
    }

    tracing::info!(interval_secs, "starting LOA expiration worker");

    let job_health = state.job_health.clone();
    crate::jobs::spawn(
        LoaExpirationJob { interval_secs },
        state,
        job_health,
        |health| &mut health.loa_expiration,
    );
}

fn loa_expiration_enabled() -> bool {
    std::env::var("LOA_EXPIRATION_ENABLED")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}

fn loa_expiration_interval_secs() -> u64 {
    std::env::var("LOA_EXPIRATION_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
        .max(60)
}
