//! Authenticated user impersonation (spec 012) — security-checklist tests.
//!
//! Covers: ACL resolves as the target, durable audit is attributed to the
//! impersonator (never the victim), facility STAFF cannot read AUTH_IMPERSONATION
//! rows, stop restores the admin, nested + SERVER_ADMIN-target are refused, a
//! self-service write is blocked while impersonating, and the retired dev login-as
//! route is gone. DB-backed; skips without `DATABASE_URL`.

mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};

use support::{TestApp, TestUser, assert_status, json_body};

/// Grants the singleton SERVER_ADMIN role to a freshly-created user (there is a
/// unique index allowing only one per database, which is fine per isolated test DB).
async fn make_server_admin(test: &TestApp, cid: i64, name: &str) -> TestUser {
    let user = test.create_user(cid, name, &[]).await;
    sqlx::query("insert into access.user_roles (user_id, role_name) values ($1, 'SERVER_ADMIN')")
        .bind(&user.id)
        .execute(&test.pool)
        .await
        .expect("grant server admin role");
    user
}

fn get_with_session(uri: &str, session_token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::COOKIE, format!("osmium_session={session_token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn impersonation_resolves_as_target_audits_impersonator_and_stop_restores() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = make_server_admin(&test, 700_001, "Admin").await;
    // Grant the target the baseline self-read every real user holds (via the USER
    // role), so the ACL-as-target check on /me is meaningful.
    let _target = test.create_user(700_002, "Target", &["auth.profile.read"]).await;

    // Start.
    let response = test
        .json_request(
            "POST",
            "/api/v1/admin/impersonate/700002",
            Some(&admin.session_token),
            Some(json!({ "reason": "support debugging" })),
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(
        body["cid"].as_i64(),
        Some(700_002),
        "start returns the target's /me"
    );
    assert_eq!(
        body["impersonation"]["impersonator_cid"].as_i64(),
        Some(700_001)
    );

    // The same cookie now resolves to the target (ACL as target), with a banner.
    let me: Value =
        json_body(test.request(get_with_session("/api/v1/me", &admin.session_token)).await).await;
    assert_eq!(me["cid"].as_i64(), Some(700_002));
    assert_eq!(
        me["impersonation"]["impersonator_cid"].as_i64(),
        Some(700_001)
    );

    // Durable audit START row is attributed to the impersonator (admin), never the victim.
    let (action, actor_user_id): (String, Option<String>) = sqlx::query_as(
        "select l.action, a.user_id \
         from access.audit_logs l \
         left join access.actors a on a.id = l.actor_id \
         where l.resource_type = 'AUTH_IMPERSONATION' \
         order by l.created_at asc limit 1",
    )
    .fetch_one(&test.pool)
    .await
    .expect("impersonation audit row");
    assert_eq!(action, "START");
    assert_eq!(actor_user_id.as_deref(), Some(admin.id.as_str()));

    // Stop restores the admin on the same session.
    let response = test
        .json_request(
            "POST",
            "/api/v1/admin/impersonate/stop",
            Some(&admin.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let restored: Value = json_body(response).await;
    assert_eq!(restored["cid"].as_i64(), Some(700_001));
    assert!(restored["impersonation"].is_null());

    let me_again: Value =
        json_body(test.request(get_with_session("/api/v1/me", &admin.session_token)).await).await;
    assert_eq!(me_again["cid"].as_i64(), Some(700_001));
    assert!(me_again["impersonation"].is_null());

    test.cleanup().await;
}

#[tokio::test]
async fn facility_staff_cannot_read_impersonation_audit_rows() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = make_server_admin(&test, 700_010, "Admin").await;
    let _target = test.create_user(700_011, "Target", &[]).await;
    let staff = test.create_user(700_012, "Staff Auditor", &["audit.logs.read"]).await;

    // Create AUTH_IMPERSONATION rows.
    test.json_request(
        "POST",
        "/api/v1/admin/impersonate/700011",
        Some(&admin.session_token),
        Some(json!({})),
    )
    .await;
    test.json_request(
        "POST",
        "/api/v1/admin/impersonate/stop",
        Some(&admin.session_token),
        None,
    )
    .await;

    // Facility staff (audit.logs.read, not SERVER_ADMIN) must not see them.
    let staff_view: Value = json_body(
        test.request(get_with_session("/api/v1/admin/audit", &staff.session_token))
            .await,
    )
    .await;
    let staff_items = staff_view["items"].as_array().expect("items");
    assert!(
        staff_items
            .iter()
            .all(|item| item["resource_type"] != "AUTH_IMPERSONATION"),
        "facility staff must never see AUTH_IMPERSONATION audit rows"
    );

    // The SERVER_ADMIN does see them.
    let admin_view: Value = json_body(
        test.request(get_with_session("/api/v1/admin/audit", &admin.session_token))
            .await,
    )
    .await;
    let admin_items = admin_view["items"].as_array().expect("items");
    assert!(
        admin_items
            .iter()
            .any(|item| item["resource_type"] == "AUTH_IMPERSONATION"),
        "server admin should see AUTH_IMPERSONATION audit rows"
    );

    test.cleanup().await;
}

#[tokio::test]
async fn refuses_impersonating_a_server_admin() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    // Actor holds the permission via a direct grant (not SERVER_ADMIN), so the
    // target can be the singleton SERVER_ADMIN — exercising the escalation guard.
    let actor = test
        .create_user(700_020, "Impersonator", &["auth.impersonate.create"])
        .await;
    let server_admin = make_server_admin(&test, 700_021, "Server Admin").await;
    let _ = &server_admin;

    let response = test
        .json_request(
            "POST",
            "/api/v1/admin/impersonate/700021",
            Some(&actor.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&response, StatusCode::FORBIDDEN);

    test.cleanup().await;
}

#[tokio::test]
async fn refuses_nested_impersonation() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = make_server_admin(&test, 700_030, "Admin").await;
    let _a = test.create_user(700_031, "Target A", &[]).await;
    let _b = test.create_user(700_032, "Target B", &[]).await;

    // Start impersonating A.
    let response = test
        .json_request(
            "POST",
            "/api/v1/admin/impersonate/700031",
            Some(&admin.session_token),
            Some(json!({})),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    // Attempting to impersonate B from the impersonating session is refused: the
    // session now resolves to A, who does not hold auth.impersonate.create.
    let response = test
        .json_request(
            "POST",
            "/api/v1/admin/impersonate/700032",
            Some(&admin.session_token),
            Some(json!({})),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    test.cleanup().await;
}

#[tokio::test]
async fn self_service_write_is_blocked_while_impersonating() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = make_server_admin(&test, 700_040, "Admin").await;
    let _target = test.create_user(700_041, "Target", &[]).await;

    test.json_request(
        "POST",
        "/api/v1/admin/impersonate/700041",
        Some(&admin.session_token),
        Some(json!({})),
    )
    .await;

    // A self-service mutation of the (victim's) profile is refused (checklist #6).
    let response = test
        .json_request(
            "PATCH",
            "/api/v1/me",
            Some(&admin.session_token),
            Some(json!({ "bio": "changed by impersonator" })),
        )
        .await;
    assert_status(&response, StatusCode::FORBIDDEN);

    test.cleanup().await;
}

#[tokio::test]
async fn admin_outbound_write_is_blocked_while_impersonating() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = make_server_admin(&test, 700_060, "Admin").await;
    // The impersonated target *holds* an admin permission, so this proves the block
    // (403) intercepts before the permission gate would have allowed it (#7).
    let _target = test
        .create_user(700_061, "Privileged Target", &["users.flags.update"])
        .await;
    let _other = test.create_user(700_062, "Other", &[]).await;

    test.json_request(
        "POST",
        "/api/v1/admin/impersonate/700061",
        Some(&admin.session_token),
        Some(json!({})),
    )
    .await;

    // An admin mutation (an outbound-side-effect-class route) is refused.
    let response = test
        .json_request(
            "PATCH",
            "/api/v1/admin/users/700062/flags",
            Some(&admin.session_token),
            Some(json!({ "no_event_signup": true, "reason": "x" })),
        )
        .await;
    assert_status(&response, StatusCode::FORBIDDEN);

    test.cleanup().await;
}

#[tokio::test]
async fn requires_the_impersonate_permission() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let regular = test.create_user(700_050, "Regular", &[]).await;
    let _target = test.create_user(700_051, "Target", &[]).await;

    let response = test
        .json_request(
            "POST",
            "/api/v1/admin/impersonate/700051",
            Some(&regular.session_token),
            Some(json!({})),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    test.cleanup().await;
}

#[tokio::test]
async fn retired_dev_login_as_route_is_gone() {
    let Some(test) = TestApp::new().await else {
        return;
    };

    let response = test
        .request(
            Request::builder()
                .uri("/api/v1/auth/login/as/700099")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_status(&response, StatusCode::NOT_FOUND);

    test.cleanup().await;
}
