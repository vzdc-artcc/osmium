mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn a_student_holds_one_pending_request_and_can_request_again_after_a_decision() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let staff = app
        .create_user(
            10000175,
            "Training Staff",
            &[
                "training.assignment_requests.create",
                "training.assignment_requests.decide",
            ],
        )
        .await;
    let student = app.create_user(10000176, "Student", &[]).await;
    let create = || {
        app.json_request(
            "POST",
            "/api/v1/training/assignment-requests",
            Some(&staff.session_token),
            Some(json!({"student_id": student.id, "submitted_at": "2026-10-04T12:00:00Z"})),
        )
    };

    let response = create().await;
    assert_status(&response, StatusCode::CREATED);
    let first: Value = json_body(response).await;

    let response = create().await;
    assert_status(&response, StatusCode::CONFLICT);

    let response = app
        .json_request(
            "PATCH",
            &format!(
                "/api/v1/training/assignment-requests/{}",
                first["id"].as_str().unwrap()
            ),
            Some(&staff.session_token),
            Some(json!({"status": "DENIED"})),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let response = create().await;
    assert_status(&response, StatusCode::CREATED);
    let second: Value = json_body(response).await;
    assert_ne!(first["id"], second["id"]);

    let rows: i64 = sqlx::query_scalar(
        "select count(*) from training.training_assignment_requests where student_id = $1",
    )
    .bind(&student.id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(rows, 2, "the denied request is kept as history");

    app.cleanup().await;
}
