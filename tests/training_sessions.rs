mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn session_crud_lifecycle_and_pi_full_replace_contract() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000094,
            "Session Staff",
            &[
                "training.sessions.create",
                "training.sessions.read",
                "training.sessions.update",
                "training.sessions.delete",
                "training.lessons.create",
                "training.lessons.update",
                "users.directory.read",
            ],
        )
        .await;
    let student = app.create_user(10000095, "Session Student", &[]).await;

    // Performance indicator template, referenced by the lesson.
    let create_template_response = app
        .json_request(
            "POST",
            "/api/v1/admin/training/performance-indicators/templates",
            Some(&staff.session_token),
            Some(json!({"name": "Radar Basics PI"})),
        )
        .await;
    assert_status(&create_template_response, StatusCode::CREATED);
    let template: Value = json_body(create_template_response).await;
    let template_id = template["id"].as_str().unwrap().to_string();

    // Lesson with a rubric and the PI template attached.
    let create_lesson_response = app
        .json_request(
            "POST",
            "/api/v1/training/lessons",
            Some(&staff.session_token),
            Some(json!({
                "identifier": "DEL1",
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
                "performance_indicator_template_id": template_id
            })),
        )
        .await;
    assert_status(&create_lesson_response, StatusCode::CREATED);
    let lesson: Value = json_body(create_lesson_response).await;
    let lesson_id = lesson["id"].as_str().unwrap().to_string();

    let create_criteria_response = app
        .json_request(
            "POST",
            &format!("/api/v1/training/lessons/{lesson_id}/rubric-criteria"),
            Some(&staff.session_token),
            Some(json!({
                "criteria": "Radar Identification",
                "description": "desc",
                "max_points": 3,
                "passing": 2
            })),
        )
        .await;
    assert_status(&create_criteria_response, StatusCode::CREATED);
    let criteria: Value = json_body(create_criteria_response).await;
    let criteria_id = criteria["id"].as_str().unwrap().to_string();

    let create_cell_response = app
        .json_request(
            "POST",
            &format!("/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells"),
            Some(&staff.session_token),
            Some(json!({"points": 3, "description": "Perfect"})),
        )
        .await;
    assert_status(&create_cell_response, StatusCode::CREATED);
    let cell: Value = json_body(create_cell_response).await;
    let cell_id = cell["id"].as_str().unwrap().to_string();

    // Create the session with a passing ticket and a PI snapshot.
    let create_session_response = app
        .json_request(
            "POST",
            "/api/v1/training/sessions",
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:00:00Z",
                "additional_comments": null,
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [{"criteria_id": criteria_id, "cell_id": cell_id, "passed": true}]
                }],
                "performance_indicator": {
                    "categories": [{
                        "name": "Basics",
                        "order": 1,
                        "criteria": [{
                            "name": "Scan",
                            "order": 1,
                            "marker": "OBSERVED",
                            "comments": null
                        }]
                    }]
                },
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&create_session_response, StatusCode::CREATED);
    let created: Value = json_body(create_session_response).await;
    let session_id = created["session"]["id"].as_str().unwrap().to_string();
    assert!(created["session"]["performance_indicator"].is_object());
    assert_eq!(
        created["session"]["performance_indicator"]["categories"][0]["criteria"][0]["marker"],
        "OBSERVED"
    );

    // The lesson requires a PI, so an update must resend it to pass validation
    // (PI follows the same "client resends what it wants to keep" full-replace
    // contract as tickets/additional_trainers) — resending the same content
    // must round-trip unchanged.
    let update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:30:00Z",
                "additional_comments": "UPDATED",
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [{"criteria_id": criteria_id, "cell_id": cell_id, "passed": true}]
                }],
                "performance_indicator": {
                    "categories": [{
                        "name": "Basics",
                        "order": 1,
                        "criteria": [{
                            "name": "Scan",
                            "order": 1,
                            "marker": "OBSERVED",
                            "comments": null
                        }]
                    }]
                },
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&update_response, StatusCode::OK);
    let updated: Value = json_body(update_response).await;
    assert_eq!(updated["session"]["additional_comments"], "UPDATED");
    assert!(
        updated["session"]["performance_indicator"].is_object(),
        "performance indicator must round-trip when resent unchanged on update, got: {updated:?}"
    );
    assert_eq!(
        updated["session"]["performance_indicator"]["categories"][0]["criteria"][0]["marker"],
        "OBSERVED"
    );

    // A lesson requiring PI rejects an update that omits it entirely (full-replace
    // contract — the caller must resend what it wants to keep).
    let missing_pi_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:30:00Z",
                "additional_comments": "UPDATED AGAIN",
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [{
                    "lesson_id": lesson_id,
                    "passed": true,
                    "scores": [{"criteria_id": criteria_id, "cell_id": cell_id, "passed": true}]
                }],
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&missing_pi_response, StatusCode::BAD_REQUEST);
    let missing_pi_body: Value = json_body(missing_pi_response).await;
    assert!(!missing_pi_body["errors"].as_array().unwrap().is_empty());

    // Update-path validation errors must surface a populated `errors` body, same as create.
    let invalid_update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            Some(json!({
                "student_id": student.id,
                "start": "2026-02-01T12:00:00Z",
                "end": "2026-02-01T13:30:00Z",
                "additional_comments": null,
                "trainer_comments": null,
                "enable_markdown": false,
                "tickets": [],
                "additional_trainers": []
            })),
        )
        .await;
    assert_status(&invalid_update_response, StatusCode::BAD_REQUEST);
    let invalid_body: Value = json_body(invalid_update_response).await;
    assert!(
        !invalid_body["errors"].as_array().unwrap().is_empty(),
        "expected populated errors array on update validation failure, got: {invalid_body:?}"
    );

    // List item ticket denormalization.
    let list_response = app
        .json_request(
            "GET",
            "/api/v1/training/sessions?page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&list_response, StatusCode::OK);
    let list: Value = json_body(list_response).await;
    let found = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == session_id)
        .expect("session present in list");
    assert_eq!(found["ticket_count"], 1);
    assert_eq!(found["tickets"][0]["lesson_identifier"], "DEL1");
    assert_eq!(found["tickets"][0]["passed"], true);

    // Roster list exposes role_names/operating_initials to a privileged caller.
    let roster_response = app
        .json_request(
            "GET",
            "/api/v1/users?page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&roster_response, StatusCode::OK);
    let roster: Value = json_body(roster_response).await;
    let staff_row = roster["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["basic"]["cid"] == 10000094)
        .expect("staff present in roster");
    assert!(staff_row["full"]["role_names"].is_array());
    assert!(
        staff_row["full"]
            .as_object()
            .unwrap()
            .contains_key("operating_initials")
    );

    // Cleanup.
    let delete_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/training/sessions/{session_id}"),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&delete_response, StatusCode::NO_CONTENT);

    app.cleanup().await;
}

/// Regression: `sort_field=end` must produce valid SQL. `end` is a reserved SQL
/// keyword, so the ORDER BY column has to be quoted (`ts."end"`); the unquoted
/// `ts.end` it once used is a syntax error that made the whole list endpoint 500.
/// This exercises the actual query end-to-end so a later refactor of the
/// `sort_column` match that drops the quoting is caught.
#[tokio::test(flavor = "current_thread")]
async fn list_sessions_sorts_by_end_keyword() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000200,
            "Sort Staff",
            &["training.sessions.read", "users.directory.read"],
        )
        .await;
    let student = app.create_user(10000201, "Sort Student", &[]).await;

    // Two sessions whose end-order differs from their start-order, so sorting by
    // `end` produces an order distinct from the default start sort.
    for (id, start, end) in [
        (
            "session-end-a",
            "2026-03-01T10:00:00Z",
            "2026-03-01T12:00:00Z",
        ),
        (
            "session-end-b",
            "2026-03-01T11:00:00Z",
            "2026-03-01T11:30:00Z",
        ),
    ] {
        sqlx::query(
            r#"
            insert into training.training_sessions (id, student_id, instructor_id, start, "end")
            values ($1, $2, $3, $4::timestamptz, $5::timestamptz)
            "#,
        )
        .bind(id)
        .bind(&student.id)
        .bind(&staff.id)
        .bind(start)
        .bind(end)
        .execute(&app.pool)
        .await
        .expect("seed training session");
    }

    // Ascending by end: B (11:30) before A (12:00). A 500 here would mean the
    // reserved-keyword quoting regressed.
    let asc = app
        .json_request(
            "GET",
            "/api/v1/training/sessions?sort_field=end&sort_order=asc&page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&asc, StatusCode::OK);
    let asc: Value = json_body(asc).await;
    let asc_ids: Vec<&str> = asc["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        asc_ids,
        vec!["session-end-b", "session-end-a"],
        "sessions must be ordered by ascending end time"
    );

    // Descending by end: A (12:00) before B (11:30).
    let desc = app
        .json_request(
            "GET",
            "/api/v1/training/sessions?sort_field=end&sort_order=desc&page_size=200",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&desc, StatusCode::OK);
    let desc: Value = json_body(desc).await;
    let desc_ids: Vec<&str> = desc["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        desc_ids,
        vec!["session-end-a", "session-end-b"],
        "sessions must be ordered by descending end time"
    );

    app.cleanup().await;
}
