use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use crate::{
    email::service::EmailActor,
    errors::ApiError,
    handlers::org::JobExecutionSummary,
    jobs::{Job, TickOutcome},
    repos::{
        audit as audit_repo,
        org::jobs as jobs_repo,
        training::appointments::{self, AppointmentSyncRow, AppointmentWarningRow},
    },
    state::AppState,
};

// Matches the live site's old `/api/update/appointments` cron cadence
// (15 minutes).
const DEFAULT_INTERVAL_SECS: u64 = 900;
const JOB_NAME: &str = "appointments_sync";
const WARNING_WINDOW_HOURS: i64 = 12;

#[derive(Debug, Clone, Default)]
pub struct AppointmentsSyncMetrics {
    pub environments_assigned: Option<i64>,
    pub warning_emails_sent: Option<i64>,
}

/// One environment-assignment decision for a single appointment. Kept
/// separate from the DB row so the assignment algorithm itself
/// (`assign_environments`) stays a pure function, independently testable
/// without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentAssignment {
    pub appointment_id: String,
    pub environment: String,
    pub double_booking: bool,
}

/// Ports the website's old `/api/update/appointments` round-robin
/// environment assignment exactly:
/// - an appointment whose lessons are *all* location=1 ("live") always
///   gets `LIVE`, and all-location=0 always gets `CLASSROOM` — no
///   conflict-checking needed for either, since those aren't scheduled
///   against the physical training-room rotation.
/// - everything else is assigned the first `training_environments` slot
///   whose most-recently-assigned appointment's end time (+ `buffer_minutes`)
///   has already passed by this appointment's start time.
/// - if every slot is still busy, the appointment is marked `DOUBLE_BOOKED`.
///
/// `appointments` must already be sorted by `start` ascending — each slot's
/// "current occupant" is whichever appointment most recently claimed it
/// while walking the list in order, exactly like the original's
/// `previousAssignments` array.
pub fn assign_environments(
    appointments: &[AppointmentSyncRow],
    training_environments: &[String],
    buffer_minutes: i64,
) -> Vec<EnvironmentAssignment> {
    let mut slot_end_times: Vec<Option<DateTime<Utc>>> = vec![None; training_environments.len()];
    let mut results = Vec::with_capacity(appointments.len());

    for appt in appointments {
        if appt.all_live {
            results.push(EnvironmentAssignment {
                appointment_id: appt.id.clone(),
                environment: "LIVE".to_string(),
                double_booking: false,
            });
            continue;
        }

        if appt.all_classroom {
            results.push(EnvironmentAssignment {
                appointment_id: appt.id.clone(),
                environment: "CLASSROOM".to_string(),
                double_booking: false,
            });
            continue;
        }

        let free_slot = slot_end_times
            .iter()
            .position(|end| end.is_none_or(|end| appt.start >= end));

        match free_slot {
            Some(idx) => {
                slot_end_times[idx] =
                    Some(appt.start + Duration::minutes(appt.duration_minutes + buffer_minutes));
                results.push(EnvironmentAssignment {
                    appointment_id: appt.id.clone(),
                    environment: training_environments[idx].clone(),
                    double_booking: false,
                });
            }
            None => {
                results.push(EnvironmentAssignment {
                    appointment_id: appt.id.clone(),
                    environment: "DOUBLE_BOOKED".to_string(),
                    double_booking: true,
                });
            }
        }
    }

    results
}

fn training_environments() -> Vec<String> {
    std::env::var("TRAINING_ENVIRONMENTS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|list| !list.is_empty())
        .unwrap_or_else(|| vec!["ERR-CONFIG".to_string()])
}

fn buffer_minutes() -> i64 {
    std::env::var("BUFFER_TIME")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(15)
}

async fn send_warning_email(
    state: &AppState,
    pool: &sqlx::PgPool,
    actor: &audit_repo::AuditActor,
    appointment: &AppointmentWarningRow,
) {
    let payload = json!({
        "student_name": appointment.student_name,
        "trainer_name": appointment.trainer_name,
        "appointment_start": appointment.start.to_rfc3339(),
    });

    let email_actor = EmailActor {
        actor_id: actor.actor_id.clone(),
        user_id: None,
        service_account_id: None,
        request_source: "job".to_string(),
    };

    let _ = state
        .email
        .enqueue_to_users(
            pool,
            email_actor,
            "training.appointment_warning".to_string(),
            payload,
            vec![
                appointment.student_id.clone(),
                appointment.trainer_id.clone(),
            ],
        )
        .await;
}

struct AppointmentsSyncJob {
    interval_secs: u64,
}

impl Job for AppointmentsSyncJob {
    type Metrics = AppointmentsSyncMetrics;

    fn name(&self) -> &'static str {
        JOB_NAME
    }

    fn interval(&self) -> StdDuration {
        StdDuration::from_secs(self.interval_secs)
    }

    async fn tick(&self, state: &AppState) -> Result<TickOutcome<Self::Metrics>, String> {
        let Some(pool) = state.db.as_ref() else {
            return Err("database unavailable for appointments sync".to_string());
        };

        let run_id = jobs_repo::create_job_run(pool, JOB_NAME)
            .await
            .map_err(|_| "failed to record appointments sync job run".to_string())?;

        let result = run_sync(state, pool).await;

        match result {
            Ok(metrics) => {
                let _ = jobs_repo::finish_job_run_success(
                    pool,
                    &run_id,
                    json!({
                        "environments_assigned": metrics.environments_assigned,
                        "warning_emails_sent": metrics.warning_emails_sent,
                    }),
                )
                .await;

                Ok(TickOutcome::success(metrics))
            }
            Err(error) => {
                let message = format!("appointments sync sweep failed: {error:?}");
                let _ = jobs_repo::finish_job_run_failure(pool, &run_id, &message).await;
                Err(message)
            }
        }
    }
}

/// Reused by both the automatic interval worker (`tick`, above) and the
/// on-demand `POST /admin/jobs/appointments_sync/run` endpoint (via
/// `execute_appointments_sync`, below) — same "one sweep implementation,
/// two triggers" pattern as `execute_loa_expiration`/`execute_solo_expiration`.
async fn run_sync(
    state: &AppState,
    pool: &sqlx::PgPool,
) -> Result<AppointmentsSyncMetrics, ApiError> {
    let now = Utc::now();

    let future_appointments = appointments::list_future_appointments_for_sync(pool, now).await?;

    let assignments = assign_environments(
        &future_appointments,
        &training_environments(),
        buffer_minutes(),
    );

    for assignment in &assignments {
        appointments::update_appointment_environment(
            pool,
            &assignment.appointment_id,
            &assignment.environment,
            assignment.double_booking,
            now,
        )
        .await?;
    }

    let warning_window = now + Duration::hours(WARNING_WINDOW_HOURS);
    let needing_warning =
        appointments::list_appointments_needing_warning_email(pool, now, warning_window).await?;

    let actor = audit_repo::resolve_audit_actor(pool, None, None)
        .await
        .unwrap_or(audit_repo::AuditActor { actor_id: None });

    let mut warning_emails_sent = 0i64;
    for appointment in &needing_warning {
        send_warning_email(state, pool, &actor, appointment).await;
        appointments::mark_warning_email_sent(pool, &appointment.id).await?;
        warning_emails_sent += 1;
    }

    Ok(AppointmentsSyncMetrics {
        environments_assigned: Some(assignments.len() as i64),
        warning_emails_sent: Some(warning_emails_sent),
    })
}

/// Manual-trigger entry point for `POST /admin/jobs/appointments_sync/run`,
/// matching the shape `run_job`'s handler expects for the other on-demand
/// jobs.
pub(crate) async fn execute_appointments_sync(
    state: &AppState,
    pool: &sqlx::PgPool,
) -> Result<JobExecutionSummary, ApiError> {
    let metrics = run_sync(state, pool).await?;

    Ok(JobExecutionSummary {
        processed: metrics.environments_assigned.unwrap_or(0)
            + metrics.warning_emails_sent.unwrap_or(0),
        details: json!({
            "environments_assigned": metrics.environments_assigned,
            "warning_emails_sent": metrics.warning_emails_sent,
        }),
    })
}

pub fn start_appointments_sync_worker(state: AppState) {
    if !appointments_sync_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.appointments_sync.enabled = false;
            health.appointments_sync.last_error = None;
        }
        tracing::info!("appointments sync worker disabled");
        return;
    }

    let interval_secs = appointments_sync_interval_secs();
    if let Ok(mut health) = state.job_health.write() {
        health.appointments_sync.enabled = true;
        health.appointments_sync.last_error = None;
    }

    tracing::info!(interval_secs, "starting appointments sync worker");

    let job_health = state.job_health.clone();
    crate::jobs::spawn(
        AppointmentsSyncJob { interval_secs },
        state,
        job_health,
        |health| &mut health.appointments_sync,
    );
}

fn appointments_sync_enabled() -> bool {
    std::env::var("APPOINTMENTS_SYNC_ENABLED")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}

fn appointments_sync_interval_secs() -> u64 {
    std::env::var("APPOINTMENTS_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
        .max(60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, start: DateTime<Utc>, duration_minutes: i64, all_live: bool, all_classroom: bool) -> AppointmentSyncRow {
        AppointmentSyncRow {
            id: id.to_string(),
            start,
            duration_minutes,
            all_live,
            all_classroom,
        }
    }

    #[test]
    fn live_and_classroom_lessons_bypass_the_rotation() {
        let base = Utc::now();
        let appointments = vec![
            row("live", base, 60, true, false),
            row("classroom", base + Duration::hours(1), 60, false, true),
        ];

        let assignments = assign_environments(&appointments, &["SBX1".to_string()], 15);

        assert_eq!(assignments[0].environment, "LIVE");
        assert!(!assignments[0].double_booking);
        assert_eq!(assignments[1].environment, "CLASSROOM");
        assert!(!assignments[1].double_booking);
    }

    #[test]
    fn non_overlapping_appointments_reuse_the_same_slot() {
        let base = Utc::now();
        let appointments = vec![
            row("a", base, 60, false, false),
            // Starts well after `a` (60 + 15 buffer = 75 min) ends.
            row("b", base + Duration::hours(2), 60, false, false),
        ];

        let assignments = assign_environments(&appointments, &["SBX1".to_string()], 15);

        assert_eq!(assignments[0].environment, "SBX1");
        assert_eq!(assignments[1].environment, "SBX1");
        assert!(!assignments[0].double_booking);
        assert!(!assignments[1].double_booking);
    }

    #[test]
    fn overlapping_appointments_overflow_to_the_next_environment() {
        let base = Utc::now();
        let appointments = vec![
            row("a", base, 60, false, false),
            // Starts only 30 minutes after `a` — still within `a`'s
            // 60+15 buffer window, so this can't reuse SBX1.
            row("b", base + Duration::minutes(30), 60, false, false),
        ];

        let assignments = assign_environments(&appointments, &["SBX1".to_string(), "SBX2".to_string()], 15);

        assert_eq!(assignments[0].environment, "SBX1");
        assert_eq!(assignments[1].environment, "SBX2");
        assert!(!assignments[1].double_booking);
    }

    #[test]
    fn exhausting_every_environment_marks_double_booked() {
        let base = Utc::now();
        let appointments = vec![
            row("a", base, 60, false, false),
            row("b", base + Duration::minutes(10), 60, false, false),
        ];

        let assignments = assign_environments(&appointments, &["SBX1".to_string()], 15);

        assert_eq!(assignments[0].environment, "SBX1");
        assert_eq!(assignments[1].environment, "DOUBLE_BOOKED");
        assert!(assignments[1].double_booking);
    }

    #[test]
    fn slot_occupant_is_always_the_most_recently_assigned_appointment() {
        let base = Utc::now();
        // Three back-to-back appointments on a single environment: each
        // new one should check against the *previous* one's end time, not
        // the very first appointment's.
        let appointments = vec![
            row("a", base, 60, false, false),
            row("b", base + Duration::hours(2), 60, false, false),
            row("c", base + Duration::hours(4), 60, false, false),
        ];

        let assignments = assign_environments(&appointments, &["SBX1".to_string()], 15);

        assert!(assignments.iter().all(|a| a.environment == "SBX1"));
        assert!(assignments.iter().all(|a| !a.double_booking));
    }
}
