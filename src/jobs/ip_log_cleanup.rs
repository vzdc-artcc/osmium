//! Retention sweep for `access.ip_request_log` (spec 011).
//!
//! Deletes rows older than `IP_REQUEST_LOG_RETENTION_DAYS` on a daily interval so
//! the high-volume request log has bounded storage growth. Separate from the drain
//! job (writes) by design.

use std::time::Duration;

use chrono::Utc;

use crate::{
    jobs::{Job, TickOutcome},
    repos::ip_request_log,
    state::AppState,
};

const JOB_NAME: &str = "ip_log_cleanup";
const INTERVAL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Default)]
pub struct IpLogCleanupMetrics {
    pub deleted: Option<u64>,
}

struct IpLogCleanupJob {
    retention_days: i64,
}

impl Job for IpLogCleanupJob {
    type Metrics = IpLogCleanupMetrics;

    fn name(&self) -> &'static str {
        JOB_NAME
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(INTERVAL_SECS)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        let Some(pool) = state.db.as_ref() else {
            return Err("database unavailable for ip request log cleanup".to_string());
        };

        let cutoff = Utc::now() - chrono::Duration::days(self.retention_days);
        match ip_request_log::delete_older_than(pool, cutoff).await {
            Ok(deleted) => Ok(TickOutcome::success(IpLogCleanupMetrics {
                deleted: Some(deleted),
            })),
            Err(error) => Err(format!("ip request log cleanup failed: {error:?}")),
        }
    }
}

pub fn start_ip_log_cleanup_worker(state: AppState) {
    if !crate::config::ip_request_log_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.ip_log_cleanup.enabled = false;
            health.ip_log_cleanup.last_error = None;
        }
        tracing::info!("ip request log cleanup disabled");
        return;
    }

    let retention_days = crate::config::ip_request_log_retention_days();
    if let Ok(mut health) = state.job_health.write() {
        health.ip_log_cleanup.enabled = true;
        health.ip_log_cleanup.last_error = None;
    }

    tracing::info!(retention_days, "starting ip request log cleanup worker");

    let job_health = state.job_health.clone();
    crate::jobs::spawn(
        IpLogCleanupJob { retention_days },
        state,
        job_health,
        |health| &mut health.ip_log_cleanup,
    );
}
