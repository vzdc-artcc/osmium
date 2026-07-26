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
    fn clear(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        unsafe {
            std::env::remove_var(key);
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

/// NONE->HOME and HOME->VISITOR transitions never touch VATUSA (only a
/// purge, i.e. a transition *to* NONE, does), so these are safe to exercise
/// through the real HTTP endpoint end-to-end.
#[tokio::test(flavor = "current_thread")]
async fn home_and_visitor_transitions_work_via_http() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000200,
            "Lifecycle Staff",
            &["users.controller_status.update"],
        )
        .await;
    let target = app.create_user(10000201, "Lifecycle Target", &[]).await;

    let to_home = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/users/{}/controller-lifecycle", target.cid),
            Some(&staff.session_token),
            Some(json!({"controller_status": "HOME"})),
        )
        .await;
    assert_status(&to_home, StatusCode::OK);
    let home_body: Value = json_body(to_home).await;
    assert_eq!(home_body["controller_status"], "HOME");
    // Fixture users already have operating initials assigned by create_user,
    // so that branch of the cleanup summary is a no-op here — only the
    // welcome-message-on-first-activation branch fires (NONE -> non-NONE).
    assert_eq!(home_body["cleanup"]["operating_initials_assigned"], false);
    assert_eq!(home_body["cleanup"]["welcome_message_enabled"], true);

    let to_visitor = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/users/{}/controller-lifecycle", target.cid),
            Some(&staff.session_token),
            Some(json!({"controller_status": "VISITOR", "artcc": "PCT"})),
        )
        .await;
    assert_status(&to_visitor, StatusCode::OK);
    let visitor_body: Value = json_body(to_visitor).await;
    assert_eq!(visitor_body["controller_status"], "VISITOR");
    assert_eq!(visitor_body["artcc"], "PCT");
    // Already non-NONE, so neither one-time cleanup branch fires again.
    assert_eq!(visitor_body["cleanup"]["welcome_message_enabled"], false);

    app.cleanup().await;
}

/// Purging (transitioning to NONE) requires the dedicated
/// users.controller_status.delete permission on top of the base
/// users.controller_status.update every caller needs, and the VATUSA
/// roster-removal call blocks *before* any local DB mutation. This test
/// never lets a real VATUSA request go out: VATUSA_API_KEY is explicitly
/// cleared, so remove_from_vatusa_roster fails fast on its own "unconfigured"
/// check (503) rather than making a network call — proving the permission
/// gate and call-ordering wiring without ever touching VATUSA's API, per
/// the lesson from the visitor-application incident earlier in this
/// migration (never exercise a real external write from an automated test).
#[tokio::test(flavor = "current_thread")]
async fn purge_transition_requires_delete_permission_and_blocks_on_vatusa() {
    let _env_lock = lock_env();
    let _vatusa_key_guard = EnvVarGuard::clear("VATUSA_API_KEY");
    let Some(app) = TestApp::new().await else {
        return;
    };

    let update_only_staff = app
        .create_user(
            10000202,
            "Update Only Staff",
            &["users.controller_status.update"],
        )
        .await;
    let atm = app
        .create_user(
            10000203,
            "ATM Staff",
            &[
                "users.controller_status.update",
                "users.controller_status.delete",
            ],
        )
        .await;
    let target = app.create_user(10000204, "Purge Target", &[]).await;

    // Get the target onto HOME first (no VATUSA call needed for that leg).
    let to_home = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/users/{}/controller-lifecycle", target.cid),
            Some(&atm.session_token),
            Some(json!({"controller_status": "HOME"})),
        )
        .await;
    assert_status(&to_home, StatusCode::OK);

    // A STAFF member with only the general update permission cannot purge.
    let denied = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/users/{}/controller-lifecycle", target.cid),
            Some(&update_only_staff.session_token),
            Some(json!({"controller_status": "NONE"})),
        )
        .await;
    assert_status(&denied, StatusCode::UNAUTHORIZED);

    // An ATM/DATM-equivalent user passes the permission gate and reaches the
    // VATUSA call, which fails safely (503) because no API key is configured
    // in this test process — never a real network request.
    let atm_attempt = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/users/{}/controller-lifecycle", target.cid),
            Some(&atm.session_token),
            Some(json!({"controller_status": "NONE"})),
        )
        .await;
    assert_status(&atm_attempt, StatusCode::SERVICE_UNAVAILABLE);

    // The failed VATUSA call must have aborted before any DB mutation —
    // the target should still be HOME.
    let status: String =
        sqlx::query_scalar("select controller_status from org.memberships where user_id = $1")
            .bind(&target.id)
            .fetch_one(&app.pool)
            .await
            .expect("fetch membership status");
    assert_eq!(status, "HOME");

    app.cleanup().await;
}

/// The NONE-transition's cleanup side effects (status update, OI clear,
/// cascading deletes) are tested directly at the repo layer rather than
/// through the HTTP handler, since going through HTTP would also attempt
/// the real VATUSA roster-removal call.
#[tokio::test(flavor = "current_thread")]
async fn controller_lifecycle_none_cleanup_repo_layer_deletes_expected_rows() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let trainer = app.create_user(10000205, "Cleanup Trainer", &[]).await;
    let student = app.create_user(10000206, "Cleanup Student", &[]).await;

    sqlx::query("update org.memberships set controller_status = 'HOME' where user_id = $1")
        .bind(&student.id)
        .execute(&app.pool)
        .await
        .expect("seed HOME status");

    sqlx::query(
        "insert into training.training_assignment_requests (id, student_id, submitted_at, status) values ($1, $2, now(), 'PENDING')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&student.id)
    .execute(&app.pool)
    .await
    .expect("insert assignment request");

    sqlx::query(
        "insert into training.training_assignments (id, student_id, primary_trainer_id) values ($1, $2, $3)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&student.id)
    .bind(&trainer.id)
    .execute(&app.pool)
    .await
    .expect("insert assignment");

    let start = Utc::now() + Duration::days(10);
    sqlx::query(
        "insert into org.loas (id, user_id, start, \"end\", reason, status) values ($1, $2, $3, $4, 'test', 'PENDING')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&student.id)
    .bind(start)
    .bind(start + Duration::days(10))
    .execute(&app.pool)
    .await
    .expect("insert loa");

    let mut tx = app.pool.begin().await.expect("begin tx");

    osmium::repos::org::controller_lifecycle::update_membership_status(
        &mut *tx,
        &student.id,
        "NONE",
        None,
    )
    .await
    .expect("update status");

    let oi_cleared =
        osmium::repos::org::controller_lifecycle::clear_operating_initials(&mut *tx, &student.id)
            .await
            .expect("clear OI");
    assert!(oi_cleared);

    let requests_deleted =
        osmium::repos::org::controller_lifecycle::delete_training_assignment_requests_for_user(
            &mut *tx,
            &student.id,
        )
        .await
        .expect("delete requests");
    assert_eq!(requests_deleted, 1);

    let assignments_deleted =
        osmium::repos::org::controller_lifecycle::delete_training_assignments_for_user(
            &mut *tx,
            &student.id,
        )
        .await
        .expect("delete assignments");
    assert_eq!(assignments_deleted, 1);

    let loas_deleted = osmium::repos::org::loas::delete_loas_for_user(&mut *tx, &student.id)
        .await
        .expect("delete loas");
    assert_eq!(loas_deleted, 1);

    tx.commit().await.expect("commit");

    let row = osmium::repos::org::controller_lifecycle::fetch_membership_lifecycle_row(
        &app.pool,
        student.cid,
    )
    .await
    .expect("fetch row")
    .expect("row exists");
    assert_eq!(row.controller_status, "NONE");
    assert!(row.operating_initials.is_none());

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn list_purge_candidates_endpoint_returns_activity_and_respects_period_bounds() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000207,
            "Purge List Staff",
            &["users.controller_status.update"],
        )
        .await;
    let controller = app
        .create_user(10000208, "Purge List Controller", &[])
        .await;
    let trainer = app.create_user(10000209, "Purge List Trainer", &[]).await;

    // join_date defaults to "now" on creation, which would fall outside the
    // 2026 query windows below (the join-date filter excludes anyone who
    // joined after the window) — backdate it so only the hours/broadcast
    // filtering is under test here.
    sqlx::query(
        "update org.memberships set controller_status = 'HOME', join_date = '2025-01-01T00:00:00Z' where user_id = $1",
    )
    .bind(&controller.id)
    .execute(&app.pool)
    .await
    .expect("seed HOME status");

    // 2 hours of live "delivery" time in March 2026 (month index 2).
    sqlx::query(
        "insert into stats.controller_monthly_rollups (environment, cid, year, month, delivery_seconds) values ('live', $1, 2026, 2, 7200)",
    )
    .bind(controller.cid)
    .execute(&app.pool)
    .await
    .expect("insert rollup");

    // A training session inside the window, received as a student.
    sqlx::query(
        "insert into training.training_sessions (id, student_id, instructor_id, start, \"end\") values ($1, $2, $3, $4, $5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&controller.id)
    .bind(&trainer.id)
    .bind(chrono::DateTime::parse_from_rfc3339("2026-03-15T00:00:00Z").unwrap())
    .bind(chrono::DateTime::parse_from_rfc3339("2026-03-15T01:00:00Z").unwrap())
    .execute(&app.pool)
    .await
    .expect("insert training session");

    // An unseen broadcast targeted at the controller, posted inside the window.
    let broadcast_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "insert into web.change_broadcasts (id, title, description, timestamp) values ($1, 'Test', 'Test', $2)",
    )
    .bind(&broadcast_id)
    .bind(chrono::DateTime::parse_from_rfc3339("2026-03-10T00:00:00Z").unwrap())
    .execute(&app.pool)
    .await
    .expect("insert broadcast");
    sqlx::query(
        "insert into web.change_broadcast_recipients (broadcast_id, user_id) values ($1, $2)",
    )
    .bind(&broadcast_id)
    .bind(&controller.id)
    .execute(&app.pool)
    .await
    .expect("insert broadcast recipient");

    let response = app
        .json_request(
            "GET",
            "/api/v1/admin/roster/purge-candidates?year=2026&start_month=0&end_month=3",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let items = body["items"].as_array().unwrap();
    let item = items
        .iter()
        .find(|i| i["cid"] == controller.cid)
        .expect("controller present in Q1 2026 window");
    assert_eq!(item["controlling_hours"], 2.0);
    assert_eq!(item["trainer_hours_received"], 1.0);
    assert_eq!(item["trainer_hours_given"], 0.0);
    assert_eq!(item["total_hours"], 3.0);
    assert_eq!(item["open_broadcasts"], 1);
    assert_eq!(item["has_active_approved_loa"], false);

    // Outside the window (a later quarter): none of that activity should count.
    let outside_response = app
        .json_request(
            "GET",
            "/api/v1/admin/roster/purge-candidates?year=2026&start_month=6&end_month=8",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&outside_response, StatusCode::OK);
    let outside_body: Value = json_body(outside_response).await;
    let outside_items = outside_body["items"].as_array().unwrap();
    let outside_item = outside_items
        .iter()
        .find(|i| i["cid"] == controller.cid)
        .expect("controller still present (still a roster member)");
    assert_eq!(outside_item["controlling_hours"], 0.0);
    assert_eq!(outside_item["trainer_hours_received"], 0.0);

    let bad_range = app
        .json_request(
            "GET",
            "/api/v1/admin/roster/purge-candidates?year=2026&start_month=5&end_month=2",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&bad_range, StatusCode::BAD_REQUEST);

    app.cleanup().await;
}
