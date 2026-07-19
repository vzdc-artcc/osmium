use std::time::Duration;

use sqlx::PgPool;

use crate::{
    email::EmailWorkerMetrics,
    jobs::{Job, TickOutcome},
    state::AppState,
};

struct EmailDeliveryJob {
    pool: PgPool,
    interval_secs: u64,
}

impl Job for EmailDeliveryJob {
    type Metrics = EmailWorkerMetrics;

    fn name(&self) -> &'static str {
        "email_delivery"
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        let result = state.email.process_pending_batch(&self.pool).await;
        let pending_count = state.email.pending_count(&self.pool).await.ok();

        match result {
            Ok(_) => Ok(TickOutcome::success(EmailWorkerMetrics { pending_count })),
            Err(err) => Err(err.to_string()),
        }
    }
}

pub fn start_email_delivery_worker(state: AppState) {
    let enabled = state.email.worker_enabled();
    if let Ok(mut health) = state.email_health.write() {
        health.worker.enabled = enabled;
    }

    if !enabled {
        tracing::info!("email delivery worker disabled");
        return;
    }

    let Some(pool) = state.db.clone() else {
        tracing::info!("email delivery worker skipped (no database configured)");
        return;
    };

    let interval_secs = state.email.config.worker_interval_secs;
    let job = EmailDeliveryJob {
        pool,
        interval_secs,
    };
    let email_health = state.email_health.clone();
    crate::jobs::spawn(job, state, email_health, |health| &mut health.worker);
}
