mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn progression_assignments_carry_the_students_legal_name() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let staff = app
        .create_user(
            10000172,
            "Training Staff",
            &["training.lessons.read", "training.lessons.update"],
        )
        .await;
    // A migrated user's display name can be their preferred name.
    let student = app.create_user(10000173, "Preferred Pete", &[]).await;
    sqlx::raw_sql(&format!(
        "update identity.users set first_name = 'Peter', last_name = 'Parker' where id = '{}';
         insert into training.training_progressions (id, name) values ('prog-s1', 'S1 Progression');",
        student.id
    ))
    .execute(&app.pool)
    .await
    .expect("seed student name and progression");

    let response = app
        .json_request(
            "POST",
            "/api/v1/admin/training/progression-assignments",
            Some(&staff.session_token),
            Some(json!({"user_id": student.id, "progression_id": "prog-s1"})),
        )
        .await;
    assert_status(&response, StatusCode::CREATED);
    let created: Value = json_body(response).await;
    assert_eq!(created["first_name"], "Peter");
    assert_eq!(created["last_name"], "Parker");
    assert_eq!(created["display_name"], "Preferred Pete");

    let response = app
        .json_request(
            "GET",
            "/api/v1/admin/training/progression-assignments",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["user_id"] == student.id.as_str())
        .expect("assignment listed");
    assert_eq!(item["first_name"], "Peter");
    assert_eq!(item["last_name"], "Parker");

    app.cleanup().await;
}
