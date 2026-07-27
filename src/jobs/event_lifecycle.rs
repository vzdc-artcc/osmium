use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::{
    email::service::EmailActor,
    jobs::{Job, TickOutcome},
    repos::org::jobs as jobs_repo,
    state::AppState,
};

const DEFAULT_INTERVAL_SECS: u64 = 300;
const DEFAULT_REMINDER_LEAD_HOURS: i64 = 24;
const JOB_NAME: &str = "event_automation";

#[derive(Debug, Clone, Default)]
pub struct EventLifecycleMetrics {
    pub positions_locked: Option<i64>,
    pub events_archived: Option<i64>,
    pub reminders_sent: Option<i64>,
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

        // Best-effort reminder dispatch — additive to the lock/archive sweep, and
        // never the reason the tick fails.
        let reminders_sent = send_due_event_reminders(state, pool, now).await;

        match (lock_result, archive_result) {
            (Ok(positions_locked), Ok(events_archived)) => {
                let _ = jobs_repo::finish_job_run_success(
                    pool,
                    &run_id,
                    serde_json::json!({
                        "positions_locked": positions_locked,
                        "events_archived": events_archived,
                        "reminders_sent": reminders_sent,
                    }),
                )
                .await;

                Ok(TickOutcome::success(EventLifecycleMetrics {
                    positions_locked: Some(positions_locked),
                    events_archived: Some(events_archived),
                    reminders_sent: Some(reminders_sent),
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

/// Dispatch "your event is coming up" reminders to controllers holding a published
/// position on any event entering the lead window. Fires once per event (guarded by
/// `events.reminder_sent_at`). Returns the number of events reminded. Best-effort:
/// individual failures are logged and skipped, never propagated.
async fn send_due_event_reminders(state: &AppState, pool: &sqlx::PgPool, now: DateTime<Utc>) -> i64 {
    let window_end = now + chrono::Duration::hours(event_reminder_lead_hours());
    let due = match jobs_repo::fetch_events_due_for_reminder(pool, window_end).await {
        Ok(events) => events,
        Err(_) => return 0,
    };

    // `unsubscribe_base_url` is this deployment's public base URL (see email/render.rs).
    let base = state.email.config.unsubscribe_base_url.clone();
    let mut reminded = 0i64;

    for event in due {
        let recipients = match jobs_repo::fetch_event_reminder_recipients(pool, &event.id).await {
            Ok(recipients) => recipients,
            Err(_) => continue,
        };
        // No published participants yet — leave the event unmarked so a reminder
        // still fires if positions get published before it starts.
        if recipients.is_empty() {
            continue;
        }

        let details_url = match base.as_deref() {
            Some(base) => format!("{}/events/{}", base.trim_end_matches('/'), event.id),
            None => format!("/events/{}", event.id),
        };
        let payload = serde_json::json!({
            "event_title": event.title,
            "starts_at": event.starts_at.to_rfc3339(),
            "details_url": details_url,
            "preheader": format!("{} is coming up", event.title),
        });
        let actor = EmailActor {
            actor_id: None,
            user_id: None,
            service_account_id: None,
            request_source: "system".to_string(),
        };

        if let Err(error) = state
            .email
            .enqueue_to_users(
                pool,
                actor,
                "events.reminder".to_string(),
                payload,
                recipients,
            )
            .await
        {
            tracing::warn!(?error, event_id = %event.id, "failed to enqueue event reminder emails");
            // Leave unmarked so the next tick retries while still in the window.
            continue;
        }

        if jobs_repo::mark_event_reminder_sent(pool, &event.id).await.is_ok() {
            reminded += 1;
        }
    }

    reminded
}

fn event_reminder_lead_hours() -> i64 {
    std::env::var("EVENT_REMINDER_LEAD_HOURS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|hours| *hours > 0)
        .unwrap_or(DEFAULT_REMINDER_LEAD_HOURS)
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
