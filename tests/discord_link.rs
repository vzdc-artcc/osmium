//! Self-service Discord link (`POST /api/v1/me/discord/link/start`) — the pending
//! OAuth-state upsert must be idempotent per user, and `auth_url` is only issued
//! when Discord OAuth is actually configured.
//!
//! Regression: the upsert conflicted on the (per-attempt) state token instead of
//! the user, so a second link-start by the same user violated the table's
//! `(system_code, entity_type, external_id)` unique constraint and 500'd — any
//! user who ever started the flow once could then never link again.

mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};

use support::{EnvVarGuard, TestApp, assert_status, json_body};

const CALLBACK: &str = "http://127.0.0.1:3900/api/v1/me/discord/link/callback";

#[tokio::test]
async fn link_start_is_idempotent_per_user() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };
    let user = test
        .create_user(575_001, "Discord Linker", &["auth.profile.read"])
        .await;

    // First start provisions the pending OAuth-state row.
    let first = test
        .json_request(
            "POST",
            "/api/v1/me/discord/link/start",
            Some(&user.session_token),
            Some(json!({ "return_url": "http://127.0.0.1:3000/profile/overview" })),
        )
        .await;
    assert_status(&first, StatusCode::OK);

    // Second start (new state token, same user) must refresh the row, not 500 on
    // the external_id unique constraint.
    let second = test
        .json_request(
            "POST",
            "/api/v1/me/discord/link/start",
            Some(&user.session_token),
            Some(json!({ "return_url": "http://127.0.0.1:3000/profile/overview" })),
        )
        .await;
    assert_status(&second, StatusCode::OK);

    test.cleanup().await;
}

#[tokio::test]
async fn link_start_requires_a_session() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };

    let response = test
        .json_request(
            "POST",
            "/api/v1/me/discord/link/start",
            None,
            Some(json!({ "return_url": "http://127.0.0.1:3000/profile/overview" })),
        )
        .await;
    assert_status(&response, StatusCode::UNAUTHORIZED);

    test.cleanup().await;
}

#[tokio::test]
async fn link_start_returns_auth_url_only_when_configured() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(&[
        ("DISCORD_CLIENT_ID", "1234567890"),
        ("DISCORD_REDIRECT_URI", CALLBACK),
    ])
    .await
    else {
        return;
    };
    let user = test
        .create_user(575_002, "Discord Configured", &["auth.profile.read"])
        .await;
    let start = || {
        test.json_request(
            "POST",
            "/api/v1/me/discord/link/start",
            Some(&user.session_token),
            Some(json!({ "return_url": "http://127.0.0.1:3000/profile/overview" })),
        )
    };

    let response = start().await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    let auth_url = body["auth_url"].as_str().expect("auth_url when configured");
    assert!(auth_url.starts_with("https://discord.com/oauth2/authorize?"));
    assert!(auth_url.contains("client_id=1234567890"));
    assert!(auth_url.contains(&format!("redirect_uri={}", urlencoding::encode(CALLBACK))));

    // `.env.example` ships the keys blank; an empty value must read as unset
    // rather than produce an authorize URL with an empty client_id.
    let blank = EnvVarGuard::set("DISCORD_CLIENT_ID", "");
    let response = start().await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert!(
        body["auth_url"].is_null(),
        "expected null auth_url, got {body}"
    );
    drop(blank);

    test.cleanup().await;
}
