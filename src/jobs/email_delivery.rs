use chrono::Utc;

use crate::state::AppState;

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

    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(state.email.config.worker_interval_secs);
        loop {
            if let Ok(mut health) = state.email_health.write() {
                health.worker.last_started_at = Some(Utc::now());
                health.worker.last_error = None;
            }

            let result = state.email.process_pending_batch(&pool).await;
            let pending_count = state.email.pending_count(&pool).await.ok();

            if let Ok(mut health) = state.email_health.write() {
                health.worker.last_finished_at = Some(Utc::now());
                health.worker.last_result_ok = Some(result.is_ok());
                match result {
                    Ok(_) => {
                        health.worker.last_success_at = Some(Utc::now());
                        health.worker.pending_count = pending_count;
                    }
                    Err(err) => {
                        health.worker.last_error = Some(err.to_string());
                    }
                }
            }

            tokio::time::sleep(interval).await;
        }
    });
}
