//! Drains the buffered IP-request-log channel and bulk-inserts (spec 011).
//!
//! The request middleware pushes lightweight entries onto a bounded channel; this
//! job flushes them to the database every `IP_REQUEST_LOG_FLUSH_SECS` (or up to
//! `IP_REQUEST_LOG_BATCH_SIZE` per tick, whichever comes first). Keeping the write
//! off the request path is the whole point — the request path only pays a single
//! non-blocking channel `send`.

use std::time::Duration;

use crate::{
    errors::ApiError,
    jobs::{Job, TickOutcome},
    repos::ip_request_log,
    state::AppState,
};

const JOB_NAME: &str = "ip_log_writer";

#[derive(Debug, Clone, Default)]
pub struct IpLogWriterMetrics {
    pub inserted: Option<usize>,
}

struct IpLogWriterJob {
    interval_secs: u64,
    batch_size: usize,
}

impl Job for IpLogWriterJob {
    type Metrics = IpLogWriterMetrics;

    fn name(&self) -> &'static str {
        JOB_NAME
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        match drain_and_insert(state, self.batch_size).await {
            Ok(inserted) => Ok(TickOutcome::success(IpLogWriterMetrics {
                inserted: Some(inserted),
            })),
            Err(_) => Ok(TickOutcome::degraded(
                IpLogWriterMetrics::default(),
                "failed to flush ip request log buffer",
            )),
        }
    }
}

/// Drains up to `batch_size` buffered entries and bulk-inserts them, returning the
/// number inserted. Public so the drain job and the DB-backed tests share exactly
/// one flush implementation (the test-only synchronous-flush hook spec 011 calls for).
pub async fn drain_and_insert(state: &AppState, batch_size: usize) -> Result<usize, ApiError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(0);
    };

    let mut batch = Vec::with_capacity(batch_size.min(1024));
    {
        let mut receiver = state.ip_log_rx.lock().await;
        while batch.len() < batch_size {
            match receiver.try_recv() {
                Ok(entry) => batch.push(entry),
                // Empty or disconnected — nothing more to drain this tick.
                Err(_) => break,
            }
        }
    }

    let count = batch.len();
    if count > 0 {
        ip_request_log::insert_batch(pool, &batch).await?;
    }
    Ok(count)
}

pub fn start_ip_log_writer_worker(state: AppState) {
    if !crate::config::ip_request_log_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.ip_log_writer.enabled = false;
            health.ip_log_writer.last_error = None;
        }
        tracing::info!("ip request log writer disabled");
        return;
    }

    let interval_secs = crate::config::ip_request_log_flush_secs();
    let batch_size = crate::config::ip_request_log_batch_size();
    if let Ok(mut health) = state.job_health.write() {
        health.ip_log_writer.enabled = true;
        health.ip_log_writer.last_error = None;
    }

    tracing::info!(interval_secs, batch_size, "starting ip request log writer");

    let job_health = state.job_health.clone();
    crate::jobs::spawn(
        IpLogWriterJob {
            interval_secs,
            batch_size,
        },
        state,
        job_health,
        |health| &mut health.ip_log_writer,
    );
}
