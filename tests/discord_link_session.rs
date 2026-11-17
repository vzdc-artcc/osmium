//! `GET /api/v1/me/discord/link/callback` completes a link only for a browser whose
//! osmium session belongs to the user who started the flow. A `state` token handed
//! to anyone else, signed in or not, links nothing.

mod support;

use axum::http::{StatusCode, header};
use serde_json::json;

use support::{EnvVarGuard, TestApp, TestUser, assert_status};

const RETURN_URL: &str = "http://127.0.0.1:3000/profile/overview";

/// Starts a link as `user` and returns the state token the start stored.
async fn start_link(test: &TestApp, user: &TestUser) -> String {
    let response = test
        .json_request(
            "POST",
            "/api/v1/me/discord/link/start",
            Some(&user.session_token),
            Some(json!({ "return_url": RETURN_URL })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    sqlx::query_scalar::<_, String>(
        "select local_id from integration.external_sync_mappings \
         where system_code = 'discord' and entity_type = 'oauth_state' and external_id = $1",
    )
    .bind(&user.id)
    .fetch_one(&test.pool)
    .await
    .expect("state row written by link start")
}

/// Calls the callback with `state` and a dummy code, returning the redirect target.
async fn callback(test: &TestApp, session_token: Option<&str>, state: &str) -> String {
    callback_with_query(
        test,
        session_token,
        &format!("code=dummy-code&state={state}"),
    )
    .await
}

async fn callback_with_query(test: &TestApp, session_token: Option<&str>, query: &str) -> String {
    let response = test
        .json_request(
            "GET",
            &format!("/api/v1/me/discord/link/callback?{query}"),
            session_token,
            None,
        )
        .await;
    assert_status(&response, StatusCode::SEE_OTHER);
    response
        .headers()
        .get(header::LOCATION)
        .expect("callback redirects")
        .to_str()
        .expect("ascii location")
        .to_string()
}

async fn discord_identity_count(test: &TestApp) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "select count(*) from integration.external_sync_mappings \
         where system_code = 'discord' and entity_type = 'user_identity'",
    )
    .fetch_one(&test.pool)
    .await
    .expect("count discord identities")
}

/// The starter's own session passes the binding and stops at the next step, missing
/// Discord config (`server_error`), without contacting Discord. Proves the refusal
/// in each test comes from the session check and not from a gate that refuses all.
async fn assert_starter_passes_binding(test: &TestApp, starter: &TestUser, state: &str) {
    let location = callback(test, Some(&starter.session_token), state).await;
    assert_eq!(location, format!("{RETURN_URL}?discord_error=server_error"));
}

fn unset_discord_config() -> [EnvVarGuard; 3] {
    [
        EnvVarGuard::unset("DISCORD_CLIENT_ID"),
        EnvVarGuard::unset("DISCORD_CLIENT_SECRET"),
        EnvVarGuard::unset("DISCORD_REDIRECT_URI"),
    ]
}

#[tokio::test]
async fn callback_refuses_another_users_session() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };
    let _discord = unset_discord_config();
    let starter = test
        .create_user(575_101, "Link Starter", &["auth.profile.read"])
        .await;
    let other = test
        .create_user(575_102, "Other Browser", &["auth.profile.read"])
        .await;
    let state = start_link(&test, &starter).await;

    let location = callback(&test, Some(&other.session_token), &state).await;
    assert_eq!(
        location,
        format!("{RETURN_URL}?discord_error=session_mismatch")
    );
    assert_eq!(discord_identity_count(&test).await, 0);

    assert_starter_passes_binding(&test, &starter, &state).await;

    test.cleanup().await;
}

#[tokio::test]
async fn callback_refuses_a_browser_with_no_session() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };
    let _discord = unset_discord_config();
    let starter = test
        .create_user(575_103, "Link Starter", &["auth.profile.read"])
        .await;
    let state = start_link(&test, &starter).await;

    let location = callback(&test, None, &state).await;
    assert_eq!(
        location,
        format!("{RETURN_URL}?discord_error=session_mismatch")
    );
    assert_eq!(discord_identity_count(&test).await, 0);

    assert_starter_passes_binding(&test, &starter, &state).await;

    test.cleanup().await;
}

#[tokio::test]
async fn callback_refuses_an_impersonating_session() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };
    let _discord = unset_discord_config();
    let starter = test
        .create_user(575_104, "Link Starter", &["auth.profile.read"])
        .await;
    let admin = test
        .create_user(575_105, "Impersonator", &["auth.impersonate.create"])
        .await;
    let state = start_link(&test, &starter).await;

    let impersonate = test
        .json_request(
            "POST",
            "/api/v1/admin/impersonate/575104",
            Some(&admin.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&impersonate, StatusCode::OK);

    // The admin's session now resolves as the starter, so only the impersonation
    // check stands between it and the link.
    let location = callback(&test, Some(&admin.session_token), &state).await;
    assert_eq!(
        location,
        format!("{RETURN_URL}?discord_error=session_mismatch")
    );
    assert_eq!(discord_identity_count(&test).await, 0);

    assert_starter_passes_binding(&test, &starter, &state).await;

    test.cleanup().await;
}

#[tokio::test]
async fn callback_checks_the_session_before_discords_error_param() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };
    let _discord = unset_discord_config();
    let starter = test
        .create_user(575_106, "Link Starter", &["auth.profile.read"])
        .await;
    let other = test
        .create_user(575_107, "Other Browser", &["auth.profile.read"])
        .await;
    let state = start_link(&test, &starter).await;

    let query = format!("error=access_denied&state={state}");

    assert_eq!(
        callback_with_query(&test, Some(&other.session_token), &query).await,
        format!("{RETURN_URL}?discord_error=session_mismatch")
    );
    // Positive control: the starter's session reaches Discord's error.
    assert_eq!(
        callback_with_query(&test, Some(&starter.session_token), &query).await,
        format!("{RETURN_URL}?discord_error=access_denied")
    );

    test.cleanup().await;
}
