pub mod email_delivery;
pub mod roster_sync;
pub mod stats_sync;

use std::{
    future::Future,
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::Utc;

use crate::state::AppState;

/// Health/status record for a single periodic background job. `M` carries whatever
/// metrics are specific to that job (e.g. rows processed, pending count).
#[derive(Debug, Clone, Default)]
pub struct JobHealthRecord<M: Default + Clone> {
    pub enabled: bool,
    pub last_started_at: Option<chrono::DateTime<Utc>>,
    pub last_finished_at: Option<chrono::DateTime<Utc>>,
    pub last_success_at: Option<chrono::DateTime<Utc>>,
    pub last_result_ok: Option<bool>,
    pub last_error: Option<String>,
    pub metrics: M,
}

/// The outcome of a single `Job::tick` invocation.
pub struct TickOutcome<M> {
    pub metrics: M,
    pub ok: bool,
    pub error: Option<String>,
}

impl<M> TickOutcome<M> {
    pub fn success(metrics: M) -> Self {
        Self {
            metrics,
            ok: true,
            error: None,
        }
    }

    pub fn degraded(metrics: M, error: impl Into<String>) -> Self {
        Self {
            metrics,
            ok: false,
            error: Some(error.into()),
        }
    }
}

/// A periodic background job. Implementors only describe how often to run and what a
/// single run does; [`spawn`] provides the ticking loop and health-record bookkeeping.
pub trait Job: Send + Sync + 'static {
    type Metrics: Default + Clone + Send + Sync + 'static;

    fn name(&self) -> &'static str;
    fn interval(&self) -> Duration;
    fn tick(
        &self,
        state: &AppState,
    ) -> impl Future<Output = Result<TickOutcome<Self::Metrics>, String>> + Send;
}

/// Spawns a periodic task driven by `job`, recording start/finish/success timestamps and
/// job-specific metrics into the health record reached by locking `health_lock` and
/// projecting into it with `project`.
pub fn spawn<J, A, P>(job: J, state: AppState, health_lock: Arc<RwLock<A>>, project: P)
where
    J: Job,
    A: Send + Sync + 'static,
    P: Fn(&mut A) -> &mut JobHealthRecord<J::Metrics> + Send + Sync + 'static,
{
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(job.interval());
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            let started_at = Utc::now();

            if let Ok(mut aggregate) = health_lock.write() {
                let health = project(&mut aggregate);
                health.last_started_at = Some(started_at);
                health.last_error = None;
            }

            match job.tick(&state).await {
                Ok(outcome) => {
                    if let Ok(mut aggregate) = health_lock.write() {
                        let health = project(&mut aggregate);
                        health.last_finished_at = Some(Utc::now());
                        health.last_result_ok = Some(outcome.ok);
                        health.last_error = outcome.error;
                        health.metrics = outcome.metrics;
                        if outcome.ok {
                            health.last_success_at = Some(Utc::now());
                        }
                    }
                }
                Err(message) => {
                    tracing::warn!(
                        job = job.name(),
                        error = message.as_str(),
                        "job tick failed"
                    );
                    if let Ok(mut aggregate) = health_lock.write() {
                        let health = project(&mut aggregate);
                        health.last_finished_at = Some(Utc::now());
                        health.last_result_ok = Some(false);
                        health.last_error = Some(message);
                    }
                }
            }
        }
    });
}
