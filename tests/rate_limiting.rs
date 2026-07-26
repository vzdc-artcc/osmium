//! Spec 010 — per-IP rate limiting with permission bypass.
//!
//! These are DB-backed (they need a real user + granted permission for the bypass
//! cases) and skip cleanly when `DATABASE_URL` is unset, matching the rest of the
//! suite. The limiter is configured tiny via env overrides (`burst = 3`) so the
//! boundary is cheap to hit.

mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};

use support::TestApp;

/// Enabled limiter with a 3-request burst so tests trip the boundary quickly.
const RL_ENV: &[(&str, &str)] = &[
    ("RATE_LIMIT_ENABLED", "true"),
    ("RATE_LIMIT_REQUESTS_PER_MIN", "60"),
    ("RATE_LIMIT_BURST", "3"),
];

const BURST: usize = 3;

fn ip_request(uri: &str, ip: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("x-forwarded-for", ip)
        .body(Body::empty())
        .unwrap()
}

fn ip_request_with_session(uri: &str, ip: &str, session_token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("x-forwarded-for", ip)
        .header(header::COOKIE, format!("osmium_session={session_token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn requests_under_the_limit_succeed() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(RL_ENV).await else {
        return;
    };

    for _ in 0..BURST {
        let response = test.request(ip_request("/health", "203.0.113.10")).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    test.cleanup().await;
}

#[tokio::test]
async fn requests_over_the_limit_return_429_without_bypass() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(RL_ENV).await else {
        return;
    };

    let ip = "203.0.113.11";
    for _ in 0..BURST {
        let response = test.request(ip_request("/health", ip)).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // The next request from the same IP is over the burst and holds no bypass.
    let response = test.request(ip_request("/health", ip)).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    test.cleanup().await;
}

#[tokio::test]
async fn distinct_ips_do_not_share_a_counter() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(RL_ENV).await else {
        return;
    };

    // Exhaust IP A entirely.
    for _ in 0..BURST {
        let response = test.request(ip_request("/health", "203.0.113.20")).await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    let throttled = test.request(ip_request("/health", "203.0.113.20")).await;
    assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);

    // A different IP starts with a full bucket.
    let response = test.request(ip_request("/health", "203.0.113.21")).await;
    assert_eq!(response.status(), StatusCode::OK);

    test.cleanup().await;
}

#[tokio::test]
async fn bypass_permission_exempts_an_over_limit_caller() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(RL_ENV).await else {
        return;
    };

    let user = test
        .create_user(555_001, "Bypass Holder", &["system_rate_limit.update"])
        .await;
    let ip = "203.0.113.30";

    // Exhaust the burst — these are still counted while under the limit.
    for _ in 0..BURST {
        let response = test
            .request(ip_request_with_session("/health", ip, &user.session_token))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Over the burst, the bypass permission keeps the caller flowing. Reaching a
    // 200 here also proves `resolve_current_user` populated CurrentUser *before*
    // the rate-limit middleware ran (otherwise the bypass check would 429).
    for _ in 0..3 {
        let response = test
            .request(ip_request_with_session("/health", ip, &user.session_token))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    test.cleanup().await;
}

#[tokio::test]
async fn authenticated_user_without_bypass_is_still_throttled() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(RL_ENV).await else {
        return;
    };

    let user = test.create_user(555_002, "No Bypass", &[]).await;
    let ip = "203.0.113.31";

    for _ in 0..BURST {
        let response = test
            .request(ip_request_with_session("/health", ip, &user.session_token))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = test
        .request(ip_request_with_session("/health", ip, &user.session_token))
        .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    test.cleanup().await;
}

#[tokio::test]
async fn disabling_the_flag_fully_disables_enforcement() {
    let _env_guard = support::lock_env();
    let Some(test) = TestApp::new_with_env_overrides(&[
        ("RATE_LIMIT_ENABLED", "false"),
        ("RATE_LIMIT_BURST", "3"),
    ])
    .await
    else {
        return;
    };

    // Well past the burst — none are throttled while disabled.
    for _ in 0..(BURST * 4) {
        let response = test.request(ip_request("/health", "203.0.113.40")).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    test.cleanup().await;
}
