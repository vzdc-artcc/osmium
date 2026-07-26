mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }

        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_deref() {
            unsafe {
                std::env::set_var(self.key, previous);
            }
        } else {
            unsafe {
                std::env::remove_var(self.key);
            }
        }
    }
}

/// Full sweep of the ported `/api/update/appointments` logic: LIVE/CLASSROOM
/// shortcuts bypass the rotation, same-shape appointments round-robin across
/// the single configured environment, an unavoidable overlap gets marked
/// `DOUBLE_BOOKED`, and only appointments inside the 12h warning window get
/// `warning_email_sent` flipped.
#[tokio::test(flavor = "current_thread")]
async fn appointments_sync_assigns_environments_and_flags_warning_emails() {
    let _env_lock = lock_env();
    // Single environment makes the double-booking case unambiguous to assert on.
    let _training_environments_guard = EnvVarGuard::set("TRAINING_ENVIRONMENTS", "SBX1");
    let _buffer_guard = EnvVarGuard::set("BUFFER_TIME", "15");

    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000095,
            "Appointments Sync Staff",
            &[
                "training.appointments.create",
                "training.lessons.create",
                "users.controller_status.update",
            ],
        )
        .await;
    let student = app
        .create_user(10000096, "Appointments Sync Student", &[])
        .await;

    let live_lesson = create_lesson(&app, &staff.session_token, "LIVE1", 1).await;
    let classroom_lesson = create_lesson(&app, &staff.session_token, "CLS1", 0).await;
    let rotation_lesson = create_lesson(&app, &staff.session_token, "ROT1", 2).await;

    let now = Utc::now();
    let live_start = now + Duration::hours(1);
    let classroom_start = now + Duration::hours(2);
    let a_start = now + Duration::hours(3);
    // Only 30 minutes after `a` — still inside its 60+15 buffer window, so
    // with a single training environment this can't help but double-book.
    let b_start = a_start + Duration::minutes(30);
    // Well outside the 12h warning window, but still due for environment
    // assignment — should reuse SBX1 since `a`/`b` are long finished by then.
    let far_start = now + Duration::hours(48);

    let live_id = create_appointment(
        &app,
        &staff.session_token,
        &student.id,
        live_start,
        &live_lesson,
    )
    .await;
    let classroom_id = create_appointment(
        &app,
        &staff.session_token,
        &student.id,
        classroom_start,
        &classroom_lesson,
    )
    .await;
    let a_id = create_appointment(
        &app,
        &staff.session_token,
        &student.id,
        a_start,
        &rotation_lesson,
    )
    .await;
    let b_id = create_appointment(
        &app,
        &staff.session_token,
        &student.id,
        b_start,
        &rotation_lesson,
    )
    .await;
    let far_id = create_appointment(
        &app,
        &staff.session_token,
        &student.id,
        far_start,
        &rotation_lesson,
    )
    .await;

    let run_response = app
        .json_request(
            "POST",
            "/api/v1/admin/jobs/appointments_sync/run",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&run_response, StatusCode::OK);
    let run: Value = json_body(run_response).await;
    assert_eq!(run["run"]["status"], "succeeded");
    assert_eq!(
        run["run"]["result_summary"]["details"]["environments_assigned"],
        5
    );
    assert_eq!(
        run["run"]["result_summary"]["details"]["warning_emails_sent"],
        4
    );

    assert_appointment_state(&app, &live_id, "LIVE", false, true).await;
    assert_appointment_state(&app, &classroom_id, "CLASSROOM", false, true).await;
    assert_appointment_state(&app, &a_id, "SBX1", false, true).await;
    assert_appointment_state(&app, &b_id, "DOUBLE_BOOKED", true, true).await;
    assert_appointment_state(&app, &far_id, "SBX1", false, false).await;

    app.cleanup().await;
}

async fn create_lesson(
    app: &TestApp,
    session_token: &str,
    identifier: &str,
    location: i64,
) -> String {
    let response = app
        .json_request(
            "POST",
            "/api/v1/training/lessons",
            Some(session_token),
            Some(json!({
                "identifier": identifier,
                "location": location,
                "name": identifier,
                "description": "desc",
                "position": "DEL",
                "facility": "PCT",
                "duration": 60,
                "trainee_preparation": null,
                "instructor_only": false,
                "notify_instructor_on_pass": false,
                "release_request_on_pass": false,
                "performance_indicator_template_id": null
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let lesson: Value = json_body(response).await;
    lesson["id"].as_str().unwrap().to_string()
}

async fn create_appointment(
    app: &TestApp,
    session_token: &str,
    student_id: &str,
    start: chrono::DateTime<Utc>,
    lesson_id: &str,
) -> String {
    let response = app
        .json_request(
            "POST",
            "/api/v1/training/appointments",
            Some(session_token),
            Some(json!({
                "student_id": student_id,
                "start": start.to_rfc3339(),
                "lesson_ids": [lesson_id],
                "notes": ""
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let created: Value = json_body(response).await;
    created["id"].as_str().unwrap().to_string()
}

async fn assert_appointment_state(
    app: &TestApp,
    appointment_id: &str,
    expected_environment: &str,
    expected_double_booking: bool,
    expected_warning_email_sent: bool,
) {
    let row: (Option<String>, bool, bool) = sqlx::query_as(
        "select environment, double_booking, warning_email_sent from training.training_appointments where id = $1",
    )
    .bind(appointment_id)
    .fetch_one(&app.pool)
    .await
    .expect("fetch appointment state");

    assert_eq!(
        row.0.as_deref(),
        Some(expected_environment),
        "environment for {appointment_id}"
    );
    assert_eq!(
        row.1, expected_double_booking,
        "double_booking for {appointment_id}"
    );
    assert_eq!(
        row.2, expected_warning_email_sent,
        "warning_email_sent for {appointment_id}"
    );
}
