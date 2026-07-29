//! Session-authenticated per-category email preferences
//! (`GET`/`PUT /api/v1/me/email-preferences`) — backs the profile "Email
//! Preferences" section. Verifies the round-trip through `email.suppressions` and
//! that transactional mail cannot be unsubscribed.
//!
//! Requires `DATABASE_URL` (CI provides Postgres); skips cleanly otherwise.

mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

fn category<'a>(prefs: &'a Value, id: &str) -> &'a Value {
    prefs["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("category {id} present"))
}

#[tokio::test]
async fn me_email_preferences_round_trip() {
    let _env = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let user = app
        .create_user(
            700100,
            "Prefs User",
            &["auth.profile.read", "auth.profile.update"],
        )
        .await;

    // Default state: every category subscribed (opt-out model); transactional locked.
    let get = app
        .json_request(
            "GET",
            "/api/v1/me/email-preferences",
            Some(&user.session_token),
            None,
        )
        .await;
    assert_status(&get, StatusCode::OK);
    let prefs: Value = json_body(get).await;
    assert_eq!(category(&prefs, "event_notifications")["subscribed"], true);
    assert_eq!(category(&prefs, "training")["subscribed"], true);
    let transactional = category(&prefs, "transactional");
    assert_eq!(transactional["subscribed"], true);
    assert_eq!(transactional["editable"], false);

    // Opt out of event notifications.
    let put = app
        .json_request(
            "PUT",
            "/api/v1/me/email-preferences",
            Some(&user.session_token),
            Some(json!({ "preferences": [{ "category": "event_notifications", "subscribed": false }] })),
        )
        .await;
    assert_status(&put, StatusCode::OK);
    let after: Value = json_body(put).await;
    assert_eq!(category(&after, "event_notifications")["subscribed"], false);

    // An active suppression row now exists for this user's email + category.
    let active: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from email.suppressions
        where category_id = 'event_notifications'
          and lower(email::text) = lower($1)
          and revoked_at is null
        "#,
    )
    .bind(format!("user-{}@example.invalid", user.cid))
    .fetch_one(&app.pool)
    .await
    .expect("query suppressions");
    assert_eq!(active, 1, "one active event_notifications suppression");

    // Re-subscribing revokes it.
    let resub = app
        .json_request(
            "PUT",
            "/api/v1/me/email-preferences",
            Some(&user.session_token),
            Some(json!({ "preferences": [{ "category": "event_notifications", "subscribed": true }] })),
        )
        .await;
    assert_status(&resub, StatusCode::OK);
    let resubbed: Value = json_body(resub).await;
    assert_eq!(
        category(&resubbed, "event_notifications")["subscribed"],
        true
    );

    // Transactional cannot be unsubscribed.
    let bad = app
        .json_request(
            "PUT",
            "/api/v1/me/email-preferences",
            Some(&user.session_token),
            Some(json!({ "preferences": [{ "category": "transactional", "subscribed": false }] })),
        )
        .await;
    assert_status(&bad, StatusCode::BAD_REQUEST);

    app.cleanup().await;
}

#[tokio::test]
async fn me_email_preferences_requires_authentication() {
    let _env = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let get = app
        .json_request("GET", "/api/v1/me/email-preferences", None, None)
        .await;
    assert_status(&get, StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}
