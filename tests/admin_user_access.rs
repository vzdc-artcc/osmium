mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

/// A caller without `access.users.update` never reaches any of the new
/// checks below — already covered by `permission_gates.rs`. These tests
/// cover the two new restrictions added on top of that base gate: a
/// required dossier reason, and diff-based restriction of added/removed
/// permissions to the acting staffer's own effective set.

#[tokio::test(flavor = "current_thread")]
async fn save_requires_non_empty_reason() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000400,
            "Access Actor",
            &["access.users.update", "auth.profile.read"],
        )
        .await;
    let target = app.create_user(10000401, "Access Target", &[]).await;

    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{}/access", target.cid),
            Some(&staff.session_token),
            Some(json!({
                "permissions": {"auth": {"profile": ["read"]}},
                "reason": "   "
            })),
        )
        .await;
    assert_status(&response, StatusCode::BAD_REQUEST);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn actor_cannot_add_permission_outside_their_own_scope() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    // Holds only the base gate permission — nothing else.
    let staff = app
        .create_user(10000402, "Limited Actor", &["access.users.update"])
        .await;
    let target = app.create_user(10000403, "Access Target", &[]).await;

    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{}/access", target.cid),
            Some(&staff.session_token),
            Some(json!({
                "permissions": {"auth": {"profile": ["read"]}},
                "reason": "attempting to grant something I don't have"
            })),
        )
        .await;
    assert_status(&response, StatusCode::FORBIDDEN);

    let direct_count: i64 = sqlx::query_scalar(
        "select count(*) from access.user_permissions up join identity.users u on u.id = up.user_id where u.cid = $1",
    )
    .bind(target.cid)
    .fetch_one(&app.pool)
    .await
    .expect("count target permissions");
    assert_eq!(direct_count, 0);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn actor_can_add_permission_within_their_own_scope_and_dossier_entry_is_recorded() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app
        .create_user(
            10000404,
            "Access Actor",
            &["access.users.update", "auth.profile.read"],
        )
        .await;
    let target = app.create_user(10000405, "Access Target", &[]).await;

    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{}/access", target.cid),
            Some(&staff.session_token),
            Some(json!({
                "permissions": {"auth": {"profile": ["read"]}},
                "reason": "granting profile read per request"
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["permissions"]["auth"]["profile"][0], "read");

    let granted: bool = sqlx::query_scalar(
        "select exists(select 1 from access.user_permissions up join identity.users u on u.id = up.user_id where u.cid = $1 and up.permission_name = 'auth.profile.read')",
    )
    .bind(target.cid)
    .fetch_one(&app.pool)
    .await
    .expect("check target permission");
    assert!(granted);

    let dossier_message: String = sqlx::query_scalar(
        "select d.message from feedback.dossier_entries d join identity.users u on u.id = d.user_id where u.cid = $1",
    )
    .bind(target.cid)
    .fetch_one(&app.pool)
    .await
    .expect("fetch dossier entry");
    assert_eq!(dossier_message, "granting profile read per request");

    let writer_cid: i64 = sqlx::query_scalar(
        "select w.cid from feedback.dossier_entries d join identity.users w on w.id = d.writer_id join identity.users u on u.id = d.user_id where u.cid = $1",
    )
    .bind(target.cid)
    .fetch_one(&app.pool)
    .await
    .expect("fetch dossier writer");
    assert_eq!(writer_cid, staff.cid);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn actor_cannot_remove_permission_outside_their_own_scope() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    // Target already directly holds a permission the acting staffer does not.
    let staff = app
        .create_user(10000406, "Limited Actor", &["access.users.update"])
        .await;
    let target = app
        .create_user(10000407, "Access Target", &["auth.profile.read"])
        .await;

    // Submits a non-empty tree that simply omits auth.profile.read —
    // attempting to remove it without holding it themselves.
    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{}/access", target.cid),
            Some(&staff.session_token),
            Some(json!({
                "permissions": {"access": {"users": ["update"]}},
                "reason": "attempting to remove something I don't have"
            })),
        )
        .await;
    assert_status(&response, StatusCode::FORBIDDEN);

    let still_granted: bool = sqlx::query_scalar(
        "select exists(select 1 from access.user_permissions up join identity.users u on u.id = up.user_id where u.cid = $1 and up.permission_name = 'auth.profile.read')",
    )
    .bind(target.cid)
    .fetch_one(&app.pool)
    .await
    .expect("check target permission unchanged");
    assert!(still_granted);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn server_admin_actor_is_unrestricted() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app.create_user(10000408, "Server Admin Actor", &[]).await;
    sqlx::query(
        "insert into access.user_roles (user_id, role_name) values ($1, 'SERVER_ADMIN') on conflict do nothing",
    )
    .bind(&staff.id)
    .execute(&app.pool)
    .await
    .expect("grant SERVER_ADMIN role");

    let target = app
        .create_user(10000409, "Access Target", &["training.lessons.read"])
        .await;

    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{}/access", target.cid),
            Some(&staff.session_token),
            Some(json!({
                "permissions": {"system": ["read"]},
                "reason": "server admin override"
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    let direct: Vec<String> = sqlx::query_scalar(
        "select up.permission_name from access.user_permissions up join identity.users u on u.id = up.user_id where u.cid = $1 order by up.permission_name",
    )
    .bind(target.cid)
    .fetch_all(&app.pool)
    .await
    .expect("fetch target permissions");
    assert_eq!(direct, vec!["system.read".to_string()]);

    app.cleanup().await;
}
