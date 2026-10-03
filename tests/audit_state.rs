mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

const CLIENT_IP: &str = "203.0.113.7";

struct AuditRow {
    before: Option<Value>,
    after: Option<Value>,
    ip: Option<String>,
}

/// Audit rows for one resource and action, oldest first.
async fn audit_rows(
    app: &TestApp,
    resource_type: &str,
    resource_id: &str,
    action: &str,
) -> Vec<AuditRow> {
    sqlx::query_as::<_, (Option<Value>, Option<Value>, Option<String>)>(
        r#"
        select before_state, after_state, host(ip_address)
        from access.audit_logs
        where resource_type = $1 and resource_id = $2 and action = $3
        order by created_at asc
        "#,
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(action)
    .fetch_all(&app.pool)
    .await
    .expect("fetch audit rows")
    .into_iter()
    .map(|(before, after, ip)| AuditRow { before, after, ip })
    .collect()
}

async fn single_audit_row(
    app: &TestApp,
    resource_type: &str,
    resource_id: &str,
    action: &str,
) -> AuditRow {
    let mut rows = audit_rows(app, resource_type, resource_id, action).await;
    assert_eq!(
        rows.len(),
        1,
        "expected one {action} {resource_type} audit row for {resource_id}"
    );
    rows.remove(0)
}

fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("snapshot is an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// Asserts both snapshots are present and are the same persisted shape.
fn assert_same_shape(row: &AuditRow) -> (&Value, &Value) {
    let before = row.before.as_ref().expect("before_state present");
    let after = row.after.as_ref().expect("after_state present");
    assert_eq!(keys(before), keys(after), "before/after share a shape");
    (before, after)
}

/// Like `TestApp::json_request`, but sends an `X-Forwarded-For` header so the
/// audit row's client IP can be asserted.
async fn json_request_from_ip(
    app: &TestApp,
    method: &str,
    uri: &str,
    session_token: &str,
    body: Option<Value>,
) -> axum::http::Response<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, format!("osmium_session={session_token}"))
        .header("x-forwarded-for", CLIENT_IP);
    let request = match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    app.request(request).await
}

async fn create_lesson(app: &TestApp, session_token: &str, identifier: &str) -> String {
    let response = app
        .json_request(
            "POST",
            "/api/v1/training/lessons",
            Some(session_token),
            Some(json!({
                "identifier": identifier,
                "location": 2,
                "name": "Basic Radar",
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

fn session_body(student_id: &str, lesson_id: &str, trainer_comments: &str) -> Value {
    json!({
        "student_id": student_id,
        "start": "2026-02-01T12:00:00Z",
        "end": "2026-02-01T13:00:00Z",
        "additional_comments": null,
        "trainer_comments": trainer_comments,
        "enable_markdown": false,
        "tickets": [{ "lesson_id": lesson_id, "passed": false, "scores": [] }],
        "additional_trainers": []
    })
}

#[tokio::test(flavor = "current_thread")]
async fn training_session_save_update_delete_record_full_snapshots_and_ip() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10009600,
            "Audit Session Staff",
            &[
                "training.sessions.create",
                "training.sessions.update",
                "training.sessions.delete",
                "training.lessons.create",
            ],
        )
        .await;
    let student = app
        .create_user(10009601, "Audit Session Student", &[])
        .await;
    let lesson_id = create_lesson(&app, &staff.session_token, "AUD1").await;

    let response = json_request_from_ip(
        &app,
        "POST",
        "/api/v1/training/sessions",
        &staff.session_token,
        Some(session_body(&student.id, &lesson_id, "first pass")),
    )
    .await;
    assert_status(&response, StatusCode::CREATED);
    let created: Value = json_body(response).await;
    let session_id = created["session"]["id"].as_str().unwrap().to_string();

    let create_row = single_audit_row(&app, "TRAINING_SESSION", &session_id, "CREATE").await;
    assert!(create_row.before.is_none(), "a create has no before_state");
    let after = create_row.after.expect("create after_state present");
    assert_eq!(after["id"], session_id.as_str());
    assert_eq!(after["trainer_comments"], "first pass");
    assert_eq!(after["tickets"][0]["lesson_id"], lesson_id.as_str());
    assert_eq!(create_row.ip.as_deref(), Some(CLIENT_IP));

    let response = json_request_from_ip(
        &app,
        "PATCH",
        &format!("/api/v1/training/sessions/{session_id}"),
        &staff.session_token,
        Some(session_body(&student.id, &lesson_id, "second pass")),
    )
    .await;
    assert_status(&response, StatusCode::OK);

    let update_row = single_audit_row(&app, "TRAINING_SESSION", &session_id, "UPDATE").await;
    let (before, after) = assert_same_shape(&update_row);
    assert_eq!(before["trainer_comments"], "first pass");
    assert_eq!(after["trainer_comments"], "second pass");
    assert_eq!(before["tickets"].as_array().unwrap().len(), 1);
    assert_eq!(after["tickets"].as_array().unwrap().len(), 1);
    assert_eq!(update_row.ip.as_deref(), Some(CLIENT_IP));

    let response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::NO_CONTENT);

    let delete_row = single_audit_row(&app, "TRAINING_SESSION", &session_id, "DELETE").await;
    assert!(delete_row.after.is_none());
    let before = delete_row.before.expect("delete before_state present");
    assert_eq!(before["trainer_comments"], "second pass");
    assert_eq!(before["student_id"], student.id.as_str());
    assert_eq!(before["tickets"][0]["lesson_id"], lesson_id.as_str());

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn admin_profile_update_records_before_and_after_profile() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(10009610, "Audit Profile Staff", &["users.flags.update"])
        .await;
    let target = app.create_user(10009611, "Audit Profile Target", &[]).await;

    let response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/admin/users/{}/profile", target.cid),
            Some(&staff.session_token),
            Some(json!({
                "preferred_name": "Renamed",
                "bio": "new bio",
                "timezone": "America/Denver"
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let row = single_audit_row(&app, "USER_PROFILE", &target.id, "UPDATE").await;
    let (before, after) = assert_same_shape(&row);
    assert_eq!(before["timezone"], "America/Chicago");
    assert!(before["preferred_name"].is_null());
    assert_eq!(after["timezone"], "America/Denver");
    assert_eq!(after["preferred_name"], "Renamed");
    // Operating initials pass through the redactor untouched.
    assert_eq!(after["operating_initials"], format!("T{}", target.cid));

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn certification_type_delete_records_the_deleted_row_as_before_state() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(10009620, "Audit Cert Staff", &["org.certifications.update"])
        .await;

    let response = app
        .json_request(
            "POST",
            "/api/v1/admin/certification-types",
            Some(&staff.session_token),
            Some(json!({
                "name": "AUDIT TWR",
                "can_solo_cert": true,
                "auto_assign_unrestricted": false,
                "certification_options": ["NONE", "UNRESTRICTED"]
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let created: Value = json_body(response).await;
    let type_id = created["id"].as_str().unwrap().to_string();

    let response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/admin/certification-types/{type_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let row = single_audit_row(&app, "CERTIFICATION_TYPE", &type_id, "DELETE").await;
    assert!(row.after.is_none(), "a delete has no after_state");
    let before = row.before.expect("delete before_state present");
    assert_eq!(before["id"], type_id.as_str());
    assert_eq!(before["name"], "AUDIT TWR");
    assert_eq!(before["can_solo_cert"], true);
    assert_eq!(
        before["certification_options"],
        json!(["NONE", "UNRESTRICTED"])
    );

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn event_position_lock_and_unlock_record_the_event_on_both_sides() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10009630,
            "Audit Event Staff",
            &["events.items.create", "events.items.update"],
        )
        .await;

    let response = app
        .json_request(
            "POST",
            "/api/v1/events",
            Some(&staff.session_token),
            Some(json!({
                "title": "Audit Event",
                "event_type": "STANDARD",
                "host": "vZDC",
                "description": "audit snapshot test",
                "starts_at": "2026-05-09T15:00:00Z",
                "ends_at": "2026-05-09T17:00:00Z"
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let event: Value = json_body(response).await;
    let event_id = event["id"].as_str().unwrap().to_string();

    for path in ["lock", "unlock"] {
        let response = app
            .json_request(
                "POST",
                &format!("/api/v1/events/{event_id}/positions/{path}"),
                Some(&staff.session_token),
                None,
            )
            .await;
        assert_status(&response, StatusCode::OK);
    }

    let rows = audit_rows(&app, "EVENT_POSITION_LOCK", &event_id, "UPDATE").await;
    assert_eq!(rows.len(), 2);

    let (before, after) = assert_same_shape(&rows[0]);
    assert_eq!(before["id"], event_id.as_str());
    assert_eq!(before["positions_locked"], false);
    assert_eq!(after["positions_locked"], true);

    let (before, after) = assert_same_shape(&rows[1]);
    assert_eq!(before["positions_locked"], true);
    assert_eq!(after["positions_locked"], false);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn access_update_records_role_names_on_both_sides() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(10009640, "Audit Access Actor", &["access.users.update"])
        .await;
    sqlx::query(
        "insert into access.user_roles (user_id, role_name) values ($1, 'SERVER_ADMIN') on conflict do nothing",
    )
    .bind(&staff.id)
    .execute(&app.pool)
    .await
    .expect("grant SERVER_ADMIN");
    let target = app.create_user(10009641, "Audit Access Target", &[]).await;

    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{}/access", target.cid),
            Some(&staff.session_token),
            Some(json!({
                "permissions": {"auth": {"profile": ["read"]}},
                "role_names": ["MTR"],
                "reason": "audit snapshot test"
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let row = single_audit_row(&app, "USER_ACCESS", &target.id, "UPDATE").await;
    let (before, after) = assert_same_shape(&row);
    assert_eq!(before["role_names"], json!([]));
    assert_eq!(after["role_names"], json!(["MTR"]));
    assert!(before["permissions"]["auth"].is_null());
    assert_eq!(after["permissions"]["auth"]["profile"], json!(["read"]));

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn rubric_criteria_update_records_persisted_rows_on_both_sides() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10009650,
            "Audit Rubric Staff",
            &["training.lessons.create", "training.lessons.update"],
        )
        .await;
    let lesson_id = create_lesson(&app, &staff.session_token, "AUD2").await;

    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/training/lessons/{lesson_id}/rubric-criteria"),
            Some(&staff.session_token),
            Some(json!({
                "criteria": "Radar Identification",
                "description": "original",
                "max_points": 3,
                "passing": 2
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let criteria: Value = json_body(response).await;
    let criteria_id = criteria["id"].as_str().unwrap().to_string();

    let response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}"),
            Some(&staff.session_token),
            Some(json!({
                "criteria": "Radar Identification",
                "description": "revised",
                "max_points": 3,
                "passing": 1
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let row = single_audit_row(&app, "LESSON_RUBRIC_CRITERIA", &criteria_id, "UPDATE").await;
    let (before, after) = assert_same_shape(&row);
    assert_eq!(before["description"], "original");
    assert_eq!(before["passing"], 2);
    assert_eq!(after["description"], "revised");
    assert_eq!(after["passing"], 1);
    assert_eq!(after["sort_order"], before["sort_order"]);

    // The create row's after_state is the same persisted row the update later
    // reads back as its before_state.
    let create_row = single_audit_row(&app, "LESSON_RUBRIC_CRITERIA", &criteria_id, "CREATE").await;
    assert!(create_row.before.is_none());
    let created_after = create_row.after.expect("create after_state present");
    assert_eq!(&created_after, before);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn appointment_update_records_persisted_detail_on_both_sides() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10009660,
            "Audit Appointment Staff",
            &[
                "training.appointments.create",
                "training.appointments.update",
                "training.lessons.create",
            ],
        )
        .await;
    let student = app
        .create_user(10009661, "Audit Appointment Student", &[])
        .await;
    let lesson_id = create_lesson(&app, &staff.session_token, "AUD3").await;

    let response = app
        .json_request(
            "POST",
            "/api/v1/training/appointments",
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "lesson_ids": [lesson_id],
                "environment": "SWEATBOX1",
                "notes": "",
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let created: Value = json_body(response).await;
    let appointment_id = created["id"].as_str().unwrap().to_string();

    let response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/appointments/{appointment_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T13:00:00Z",
                "lesson_ids": [lesson_id],
                "notes": "UPDATED"
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let row = single_audit_row(&app, "TRAINING_APPOINTMENT", &appointment_id, "UPDATE").await;
    let (before, after) = assert_same_shape(&row);
    assert_eq!(before["notes"], "");
    assert_eq!(after["notes"], "UPDATED");
    assert_eq!(before["environment"], "SWEATBOX1");
    assert!(after["environment"].is_null());
    assert_eq!(after["student_name"], "Audit Appointment Student");
    assert_eq!(after["lessons"][0]["id"], lesson_id.as_str());

    app.cleanup().await;
}
