mod support;

use axum::http::StatusCode;
use serde_json::Value;
use support::{TestApp, assert_status, env_test_lock, json_body};

#[tokio::test(flavor = "current_thread")]
async fn me_resolves_current_user_from_db_session() {
    let _env_lock = env_test_lock().lock().unwrap();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let user = app
        .create_user(10000011, "Session User", &["auth.profile.read"])
        .await;

    let response = app
        .json_request("GET", "/api/v1/me", Some(&user.session_token), None)
        .await;
    assert_status(&response, StatusCode::OK);

    let body: Value = json_body(response).await;
    assert_eq!(body["cid"], 10000011);
    assert_eq!(body["display_name"], "Session User");

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn deleted_session_is_rejected() {
    let _env_lock = env_test_lock().lock().unwrap();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let user = app
        .create_user(10000012, "Expired Session User", &["auth.profile.read"])
        .await;

    sqlx::query("delete from identity.sessions where session_token = $1")
        .bind(&user.session_token)
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app
        .json_request("GET", "/api/v1/me", Some(&user.session_token), None)
        .await;
    assert_status(&response, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn logout_removes_session_and_blocks_future_requests() {
    let _env_lock = env_test_lock().lock().unwrap();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let user = app
        .create_user(
            10000013,
            "Logout Session User",
            &["auth.profile.read", "auth.sessions.delete"],
        )
        .await;

    let logout_response = app
        .json_request(
            "POST",
            "/api/v1/auth/logout",
            Some(&user.session_token),
            None,
        )
        .await;
    assert_status(&logout_response, StatusCode::NO_CONTENT);

    let db_session_count: i64 = sqlx::query_scalar(
        "select count(*)::bigint from identity.sessions where session_token = $1",
    )
    .bind(&user.session_token)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(db_session_count, 0);

    let me_response = app
        .json_request("GET", "/api/v1/me", Some(&user.session_token), None)
        .await;
    assert_status(&me_response, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}
