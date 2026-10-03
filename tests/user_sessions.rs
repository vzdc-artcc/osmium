//! Admin user session manager — list + revoke over identity.sessions.
//! DB-backed; skips without `DATABASE_URL`.

mod support;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use serde_json::Value;
use uuid::Uuid;

use support::{TestApp, TestUser, assert_status, json_body};

fn admin_request(method: &str, uri: &str, session_token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, format!("osmium_session={session_token}"))
        .body(Body::empty())
        .unwrap()
}

/// Inserts an extra active session row for a user, returning its token.
async fn add_session(test: &TestApp, user: &TestUser) -> String {
    let token = Uuid::new_v4().to_string();
    sqlx::query(
        "insert into identity.sessions (session_token, user_id, ip_address, expires_at) \
         values ($1, $2, '203.0.113.5'::inet, now() + interval '30 days')",
    )
    .bind(&token)
    .bind(&user.id)
    .execute(&test.pool)
    .await
    .expect("insert extra session");
    token
}

async fn me_status(test: &TestApp, token: &str) -> StatusCode {
    test.request(admin_request("GET", "/api/v1/me", token))
        .await
        .status()
}

#[tokio::test]
async fn lists_sessions_without_exposing_tokens() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = test
        .create_user(570_001, "Admin", &["users.sessions.read"])
        .await;
    let target = test
        .create_user(570_002, "Target", &["auth.profile.read"])
        .await;
    add_session(&test, &target).await;

    let response = test
        .request(admin_request(
            "GET",
            "/api/v1/admin/users/570002/sessions",
            &admin.session_token,
        ))
        .await;
    assert_status(&response, StatusCode::OK);

    let body: Value = json_body(response).await;
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2, "target has two active sessions");
    for item in items {
        assert!(item.get("id").is_some());
        assert!(
            item.get("session_token").is_none(),
            "raw session tokens must never be exposed"
        );
    }

    test.cleanup().await;
}

#[tokio::test]
async fn revoking_one_session_invalidates_only_that_session() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = test
        .create_user(
            570_010,
            "Admin",
            &["users.sessions.read", "users.sessions.delete"],
        )
        .await;
    // Grant the baseline self-read so a *valid* session returns 200 on /me — otherwise
    // /me would 401 on permissions, masking whether the session itself is still live.
    let target = test
        .create_user(570_011, "Target", &["auth.profile.read"])
        .await;
    let extra = add_session(&test, &target).await;

    // Find the session id of `extra` to revoke it specifically.
    let extra_id: String =
        sqlx::query_scalar("select id from identity.sessions where session_token = $1")
            .bind(&extra)
            .fetch_one(&test.pool)
            .await
            .unwrap();

    let response = test
        .request(admin_request(
            "DELETE",
            &format!("/api/v1/admin/users/570011/sessions/{extra_id}"),
            &admin.session_token,
        ))
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);

    // The revoked token no longer authenticates; the other one still does.
    assert_eq!(me_status(&test, &extra).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        me_status(&test, &target.session_token).await,
        StatusCode::OK
    );

    // Revoking an unknown session id is a 404.
    let response = test
        .request(admin_request(
            "DELETE",
            "/api/v1/admin/users/570011/sessions/does-not-exist",
            &admin.session_token,
        ))
        .await;
    assert_status(&response, StatusCode::NOT_FOUND);

    test.cleanup().await;
}

#[tokio::test]
async fn revoke_all_clears_every_session() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    let admin = test
        .create_user(570_020, "Admin", &["users.sessions.delete"])
        .await;
    let target = test.create_user(570_021, "Target", &[]).await;
    add_session(&test, &target).await;
    add_session(&test, &target).await;

    let response = test
        .request(admin_request(
            "DELETE",
            "/api/v1/admin/users/570021/sessions",
            &admin.session_token,
        ))
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 0);

    // The target's original session is now dead too.
    assert_eq!(
        me_status(&test, &target.session_token).await,
        StatusCode::UNAUTHORIZED
    );

    test.cleanup().await;
}

#[tokio::test]
async fn session_endpoints_are_permission_gated() {
    let Some(test) = TestApp::new().await else {
        return;
    };
    // A user with no session permissions.
    let nobody = test
        .create_user(570_030, "Nobody", &["auth.profile.read"])
        .await;
    let _target = test.create_user(570_031, "Target", &[]).await;

    let list = test
        .request(admin_request(
            "GET",
            "/api/v1/admin/users/570031/sessions",
            &nobody.session_token,
        ))
        .await;
    assert_eq!(list.status(), StatusCode::FORBIDDEN);

    let revoke_all = test
        .request(admin_request(
            "DELETE",
            "/api/v1/admin/users/570031/sessions",
            &nobody.session_token,
        ))
        .await;
    assert_eq!(revoke_all.status(), StatusCode::FORBIDDEN);

    // Unauthenticated too.
    let anon = test
        .request(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/admin/users/570031/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);

    test.cleanup().await;
}
