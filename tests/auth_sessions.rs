mod support;

use axum::http::{StatusCode, header};
use serde_json::Value;
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn me_resolves_current_user_from_db_session() {
    let _env_lock = lock_env();
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
    let _env_lock = lock_env();
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
async fn logout_clears_session_cookie() {
    let _env_lock = lock_env();
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
    let cleared_cookie = logout_response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(cleared_cookie.contains("osmium_session="));
    assert!(cleared_cookie.contains("Max-Age=0"));

    app.cleanup().await;
}
