//! Spec 011 — durable IP request tracking.
//!
//! DB-backed; skips cleanly without `DATABASE_URL`. Exercises the full path:
//! middleware buffers → synchronous flush (`drain_and_insert`) → rows in
//! `access.ip_request_log` → the admin read endpoint. Also covers the retention
//! delete and the endpoint's permission gate.

mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{Duration, Utc};

use osmium::{
    jobs::ip_log_writer::drain_and_insert,
    repos::ip_request_log::{self, IpRequestLogEntry},
};
use support::{TestApp, assert_status, json_body};

const ENABLED_ENV: &[(&str, &str)] = &[("IP_REQUEST_LOG_ENABLED", "true")];

fn ip_request(uri: &str, ip: &str, session_token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri).header("x-forwarded-for", ip);
    if let Some(token) = session_token {
        builder = builder.header(header::COOKIE, format!("osmium_session={token}"));
    }
    builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn requests_are_buffered_and_flushed_with_resolved_actor() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(ENABLED_ENV).await else {
        return;
    };

    let user = test.create_user(556_001, "Log User", &[]).await;

    // Authenticated request (cookie present) — records actor_type='user'. /health is
    // public, so resolve_current_user still attaches the user without any permission.
    let user_ip = "203.0.113.50";
    let response = test
        .request(ip_request("/health", user_ip, Some(&user.session_token)))
        .await;
    assert_status(&response, StatusCode::OK);

    // Anonymous request — no actor.
    let anon_ip = "198.51.100.5";
    let response = test.request(ip_request("/health", anon_ip, None)).await;
    assert_status(&response, StatusCode::OK);

    // Force a synchronous flush of the buffer (the test-only hook spec 011 calls for).
    let inserted = drain_and_insert(&test.state, 100)
        .await
        .expect("flush ip log buffer");
    assert!(inserted >= 2, "expected at least the two requests, got {inserted}");

    // The authenticated request resolved to the user's actor.
    let (matched_path, status_code, actor_id): (String, i16, Option<String>) = sqlx::query_as(
        "select matched_path, status_code, actor_id from access.ip_request_log \
         where ip_address = $1::inet order by created_at desc limit 1",
    )
    .bind(user_ip)
    .fetch_one(&test.pool)
    .await
    .expect("user ip row");
    assert_eq!(matched_path, "/health");
    assert_eq!(status_code, 200);
    let actor_id = actor_id.expect("user request should resolve an actor_id");
    let actor_user_id: String =
        sqlx::query_scalar("select user_id from access.actors where id = $1")
            .bind(&actor_id)
            .fetch_one(&test.pool)
            .await
            .expect("actor row");
    assert_eq!(actor_user_id, user.id);

    // The anonymous request landed with a null actor.
    let anon_actor: Option<String> = sqlx::query_scalar(
        "select actor_id from access.ip_request_log where ip_address = $1::inet limit 1",
    )
    .bind(anon_ip)
    .fetch_one(&test.pool)
    .await
    .expect("anon ip row");
    assert!(anon_actor.is_none(), "anonymous request must have null actor_id");

    test.cleanup().await;
}

#[tokio::test]
async fn delete_older_than_prunes_only_aged_rows() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(ENABLED_ENV).await else {
        return;
    };

    let now = Utc::now();
    let old = IpRequestLogEntry {
        ip_address: "203.0.113.60".to_string(),
        method: "GET".to_string(),
        matched_path: "/old".to_string(),
        status_code: 200,
        actor_type: None,
        actor_ref: None,
        created_at: now - Duration::days(45),
    };
    let fresh = IpRequestLogEntry {
        created_at: now,
        matched_path: "/fresh".to_string(),
        ..old.clone()
    };
    ip_request_log::insert_batch(&test.pool, &[old, fresh])
        .await
        .expect("seed rows");

    let deleted = ip_request_log::delete_older_than(&test.pool, now - Duration::days(30))
        .await
        .expect("cleanup");
    assert_eq!(deleted, 1);

    let remaining: Vec<String> =
        sqlx::query_scalar("select matched_path from access.ip_request_log order by matched_path")
            .fetch_all(&test.pool)
            .await
            .expect("remaining rows");
    assert_eq!(remaining, vec!["/fresh".to_string()]);

    test.cleanup().await;
}

#[tokio::test]
async fn admin_ip_history_endpoint_is_permission_gated_and_scoped() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(ENABLED_ENV).await else {
        return;
    };

    let admin = test
        .create_user(556_010, "Admin", &["users.directory_private.read"])
        .await;
    let target = test.create_user(556_011, "Target", &[]).await;

    // Generate a log entry attributed to the target, then flush it.
    let response = test
        .request(ip_request("/health", "203.0.113.70", Some(&target.session_token)))
        .await;
    assert_status(&response, StatusCode::OK);
    drain_and_insert(&test.state, 100)
        .await
        .expect("flush ip log buffer");

    // Unauthenticated → 401.
    let response = test
        .request(ip_request(
            "/api/v1/admin/users/556011/ip-history",
            "203.0.113.71",
            None,
        ))
        .await;
    assert_status(&response, StatusCode::UNAUTHORIZED);

    // Authenticated but without the permission → 401.
    let response = test
        .request(ip_request(
            "/api/v1/admin/users/556011/ip-history",
            "203.0.113.72",
            Some(&target.session_token),
        ))
        .await;
    assert_status(&response, StatusCode::UNAUTHORIZED);

    // Admin with the permission → 200 and sees the target's history.
    let response = test
        .request(ip_request(
            "/api/v1/admin/users/556011/ip-history",
            "203.0.113.73",
            Some(&admin.session_token),
        ))
        .await;
    assert_status(&response, StatusCode::OK);

    let body: serde_json::Value = json_body(response).await;
    let items = body["items"].as_array().expect("items array");
    assert!(
        items.iter().any(|item| item["matched_path"] == "/health"),
        "target's /health request should appear in their ip-history"
    );
    assert!(body["pagination"]["total"].as_i64().unwrap_or(0) >= 1);

    test.cleanup().await;
}
