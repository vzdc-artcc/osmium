//! GDPR self-service data export (`GET /me/data-export`) — success shape + the
//! dedicated per-user rate limit. DB-backed; skips without `DATABASE_URL`.
//!
//! (This endpoint previously had no DB-backed test, which is how a `scope_type`
//! CHECK-constraint violation in its self-audit shipped unnoticed.)

mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::Value;

use support::{TestApp, assert_status, json_body};

fn export_request(session_token: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/me/data-export")
        .header(header::COOKIE, format!("osmium_session={session_token}"))
        .header("x-forwarded-for", "203.0.113.90")
        .body(Body::empty())
        .unwrap()
}

fn roster_export_request(session_token: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/admin/data-export/roster")
        .header(header::COOKIE, format!("osmium_session={session_token}"))
        .header("x-forwarded-for", "203.0.113.91")
        .body(Body::empty())
        .unwrap()
}

/// `create_user` seeds `controller_status = 'NONE'` (off-roster). Put a user on the
/// roster so the mass export's roster predicate includes them.
async fn set_on_roster(test: &TestApp, user_id: &str) {
    sqlx::query("update org.memberships set controller_status = 'HOME' where user_id = $1")
        .bind(user_id)
        .execute(&test.pool)
        .await
        .expect("set controller on roster");
}

#[tokio::test]
async fn export_returns_the_full_document() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };
    let user = test
        .create_user(560_001, "Export User", &["auth.profile.read"])
        .await;

    let response = test.request(export_request(&user.session_token)).await;
    assert_status(&response, StatusCode::OK);

    let body: Value = json_body(response).await;
    assert_eq!(body["meta"]["subject_cid"].as_i64(), Some(560_001));
    assert_eq!(body["meta"]["format"].as_str(), Some("json"));
    assert!(body["meta"]["gdpr_notice"].is_object());
    // Every top-level domain section is present.
    for key in [
        "identity",
        "training",
        "certifications",
        "events",
        "feedback",
        "incidents",
        "workflows",
        "notifications",
        "visitor_application",
        "activity_log",
    ] {
        assert!(body.get(key).is_some(), "missing export section: {key}");
    }

    test.cleanup().await;
}

#[tokio::test]
async fn export_is_rate_limited_per_user() {
    let _env = support::lock_env();
    // Tight dedicated limit: burst 2, and a slow hourly refill so the 3rd rapid
    // request is over. The global per-IP limiter stays off so it isn't the cause.
    let Some(test) = TestApp::new_with_env_overrides(&[
        ("RATE_LIMIT_ENABLED", "true"),
        ("RATE_LIMIT_REQUESTS_PER_MIN", "6000"),
        ("RATE_LIMIT_BURST", "6000"),
        ("DATA_EXPORT_RATE_LIMIT_PER_HOUR", "1"),
        ("DATA_EXPORT_RATE_LIMIT_BURST", "2"),
    ])
    .await else {
        return;
    };
    let user = test
        .create_user(560_002, "Spammer", &["auth.profile.read"])
        .await;

    // Burst allowance succeeds.
    for _ in 0..2 {
        let response = test.request(export_request(&user.session_token)).await;
        assert_status(&response, StatusCode::OK);
    }

    // The next rapid export is over the per-user burst.
    let response = test.request(export_request(&user.session_token)).await;
    assert_status(&response, StatusCode::TOO_MANY_REQUESTS);

    test.cleanup().await;
}

#[tokio::test]
async fn roster_export_includes_roster_controllers_without_leaking_tokens() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };

    // `users.data_export.read` is granted to no role (SERVER_ADMIN-only); granting
    // it directly to a test user is how we exercise an admin holding it.
    let admin = test
        .create_user(560_010, "Export Admin", &["users.data_export.read"])
        .await;
    let controller = test
        .create_user(560_011, "On Roster", &["auth.profile.read"])
        .await;
    set_on_roster(&test, &controller.id).await;

    let response = test.request(roster_export_request(&admin.session_token)).await;
    assert_status(&response, StatusCode::OK);

    let body: Value = json_body(response).await;
    assert!(body["gdpr_notice"].is_object());

    let subjects = body["subjects"].as_array().expect("subjects array");
    // subject_count is consistent with the array length.
    assert_eq!(body["subject_count"].as_i64(), Some(subjects.len() as i64));

    // The on-roster controller is present; each subject is a full export document.
    let cids: Vec<i64> = subjects
        .iter()
        .filter_map(|s| s["meta"]["subject_cid"].as_i64())
        .collect();
    assert!(
        cids.contains(&560_011),
        "on-roster controller missing from mass export"
    );
    let included = subjects
        .iter()
        .find(|s| s["meta"]["subject_cid"].as_i64() == Some(560_011))
        .unwrap();
    assert!(included.get("identity").is_some());
    assert!(included.get("activity_log").is_some());

    // No raw session token appears anywhere in the payload.
    let raw = serde_json::to_string(&body).unwrap();
    assert!(!raw.contains(&controller.session_token));
    assert!(!raw.contains(&admin.session_token));

    test.cleanup().await;
}

#[tokio::test]
async fn roster_export_is_permission_gated() {
    let _env = support::lock_env();
    let Some(test) = TestApp::new().await else {
        return;
    };

    // Holds the self-service export permission but NOT the admin mass-export one.
    let nobody = test
        .create_user(560_020, "Nobody", &["auth.profile.read"])
        .await;

    let response = test.request(roster_export_request(&nobody.session_token)).await;
    assert_status(&response, StatusCode::UNAUTHORIZED);

    test.cleanup().await;
}
