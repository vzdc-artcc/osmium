//! Self-service Discord link (`POST /api/v1/me/discord/link/start`) — the pending
//! OAuth-state upsert must be idempotent per user.
//!
//! Regression: the upsert conflicted on the (per-attempt) state token instead of
//! the user, so a second link-start by the same user violated the table's
//! `(system_code, entity_type, external_id)` unique constraint and 500'd — any
//! user who ever started the flow once could then never link again.

mod support;

use axum::http::StatusCode;
use serde_json::json;

use support::{TestApp, assert_status};

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
            Some(json!({ "redirect_uri": "http://localhost:3000/api/discord/callback" })),
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
            Some(json!({ "redirect_uri": "http://localhost:3000/api/discord/callback" })),
        )
        .await;
    assert_status(&second, StatusCode::OK);

    test.cleanup().await;
}
