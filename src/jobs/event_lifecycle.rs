use std::time::Duration;

use chrono::Utc;

use crate::{
    jobs::{Job, TickOutcome},
    repos::org::jobs as jobs_repo,
    state::AppState,
};

const DEFAULT_INTERVAL_SECS: u64 = 300;
const JOB_NAME: &str = "event_automation";

#[derive(Debug, Clone, Default)]
pub struct EventLifecycleMetrics {
    pub positions_locked: Option<i64>,
    pub events_archived: Option<i64>,
}

struct EventLifecycleJob {
    interval_secs: u64,
}

impl Job for EventLifecycleJob {
    type Metrics = EventLifecycleMetrics;

    fn name(&self) -> &'static str {
        JOB_NAME
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        let Some(pool) = state.db.as_ref() else {
            return Err("database unavailable for event lifecycle sweep".to_string());
        };

        // Reuses the same job_runs history the on-demand `POST
        // /admin/jobs/event_automation/run` endpoint writes to, so the admin
        // jobs UI shows automatic and manually-triggered sweeps together.
        let run_id = jobs_repo::create_job_run(pool, JOB_NAME)
            .await
            .map_err(|_| "failed to record event lifecycle job run".to_string())?;

        let now = Utc::now();
        let lock_result =
            jobs_repo::lock_events_near_start(pool, now + chrono::Duration::hours(24)).await;
        let archive_result =
            jobs_repo::archive_ended_events(pool, now - chrono::Duration::hours(24)).await;

        match (lock_result, archive_result) {
            (Ok(positions_locked), Ok(events_archived)) => {
                let _ = jobs_repo::finish_job_run_success(
                    pool,
                    &run_id,
                    serde_json::json!({
                        "positions_locked": positions_locked,
                        "events_archived": events_archived,
                    }),
                )
                .await;

                Ok(TickOutcome::success(EventLifecycleMetrics {
                    positions_locked: Some(positions_locked),
                    events_archived: Some(events_archived),
                }))
            }
            _ => {
                let _ = jobs_repo::finish_job_run_failure(
                    pool,
                    &run_id,
                    "event lifecycle sweep failed",
                )
                .await;
                Err("event lifecycle sweep failed".to_string())
            }
        }
    }
}

pub fn start_event_lifecycle_worker(state: AppState) {
    if !event_lifecycle_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.event_automation.enabled = false;
            health.event_automation.last_error = None;
        }
        tracing::info!("event lifecycle worker disabled");
        return;
    }

    let interval_secs = event_lifecycle_interval_secs();
    if let Ok(mut health) = state.job_health.write() {
        health.event_automation.enabled = true;
        health.event_automation.last_error = None;
    }

    tracing::info!(interval_secs, "starting event lifecycle worker");

    let job_health = state.job_health.clone();
    crate::jobs::spawn(
        EventLifecycleJob { interval_secs },
        state,
        job_health,
        |health| &mut health.event_automation,
    );
}

fn event_lifecycle_enabled() -> bool {
    std::env::var("EVENT_LIFECYCLE_ENABLED")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}

fn event_lifecycle_interval_secs() -> u64 {
    std::env::var("EVENT_LIFECYCLE_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
        .max(60)
}
