mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

const PERMISSION: &str = "users.operating_initials.update";

async fn set_initials(app: &TestApp, user_id: &str, initials: &str) {
    sqlx::query("update org.memberships set operating_initials = $2 where user_id = $1")
        .bind(user_id)
        .bind(initials)
        .execute(&app.pool)
        .await
        .expect("set operating initials");
}

async fn delete_membership(app: &TestApp, user_id: &str) {
    sqlx::query("delete from org.memberships where user_id = $1")
        .bind(user_id)
        .execute(&app.pool)
        .await
        .expect("delete membership");
}

async fn stored_initials(app: &TestApp, user_id: &str) -> Option<String> {
    sqlx::query_scalar("select operating_initials from org.memberships where user_id = $1")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .expect("fetch operating initials")
}

async fn membership_count(app: &TestApp, user_id: &str) -> i64 {
    sqlx::query_scalar("select count(*) from org.memberships where user_id = $1")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .expect("count memberships")
}

async fn initials_audit_count(app: &TestApp, user_id: &str) -> i64 {
    sqlx::query_scalar(
        r#"
        select count(*)
        from access.audit_logs
        where resource_type = 'USER_OPERATING_INITIALS' and resource_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&app.pool)
    .await
    .expect("count operating initials audit rows")
}

async fn reassign(
    app: &TestApp,
    token: &str,
    cid: i64,
    initials: &str,
) -> axum::response::Response {
    app.json_request(
        "PATCH",
        &format!("/api/v1/admin/users/{cid}/operating-initials"),
        Some(token),
        Some(json!({ "operating_initials": initials })),
    )
    .await
}

#[tokio::test(flavor = "current_thread")]
async fn admin_reassign_without_membership_is_not_found_and_writes_nothing() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app.create_user(10017900, "OI Staff", &[PERMISSION]).await;
    let orphan = app.create_user(10017901, "No Membership", &[]).await;
    let member = app.create_user(10017902, "Has Membership", &[]).await;
    delete_membership(&app, &orphan.id).await;
    set_initials(&app, &member.id, "MA").await;

    let response = reassign(&app, &staff.session_token, orphan.cid, "QQ").await;
    assert_status(&response, StatusCode::NOT_FOUND);
    let body: Value = json_body(response).await;
    assert_eq!(body["error"], "not_found");
    assert_eq!(membership_count(&app, &orphan.id).await, 0);
    assert_eq!(initials_audit_count(&app, &orphan.id).await, 0);

    // Positive control: the same caller and request shape succeed, store the
    // pair and audit it when the target has a membership row.
    let response = reassign(&app, &staff.session_token, member.cid, "QQ").await;
    assert_status(&response, StatusCode::OK);
    assert_eq!(
        stored_initials(&app, &member.id).await.as_deref(),
        Some("QQ")
    );
    assert_eq!(initials_audit_count(&app, &member.id).await, 1);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn admin_reassign_of_a_taken_pair_is_conflict_and_changes_nothing() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app.create_user(10017910, "OI Staff", &[PERMISSION]).await;
    let holder = app.create_user(10017911, "Holder", &[]).await;
    let target = app.create_user(10017912, "Target", &[]).await;
    set_initials(&app, &holder.id, "AA").await;
    set_initials(&app, &target.id, "BB").await;

    let response = reassign(&app, &staff.session_token, target.cid, "aa").await;
    assert_status(&response, StatusCode::CONFLICT);
    let body: Value = json_body(response).await;
    assert_eq!(body["error"], "conflict");
    assert_eq!(
        stored_initials(&app, &holder.id).await.as_deref(),
        Some("AA")
    );
    assert_eq!(
        stored_initials(&app, &target.id).await.as_deref(),
        Some("BB")
    );
    assert_eq!(initials_audit_count(&app, &target.id).await, 0);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn admin_reassign_requires_the_operating_initials_permission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let outsider = app.create_user(10017920, "No Permission", &[]).await;
    let staff = app.create_user(10017921, "OI Staff", &[PERMISSION]).await;
    let target = app.create_user(10017922, "Target", &[]).await;
    set_initials(&app, &target.id, "BB").await;

    let response = reassign(&app, &outsider.session_token, target.cid, "CC").await;
    assert_status(&response, StatusCode::UNAUTHORIZED);
    assert_eq!(
        stored_initials(&app, &target.id).await.as_deref(),
        Some("BB")
    );

    let response = reassign(&app, &staff.session_token, target.cid, "CC").await;
    assert_status(&response, StatusCode::OK);
    assert_eq!(
        stored_initials(&app, &target.id).await.as_deref(),
        Some("CC")
    );

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn self_service_initials_without_membership_is_not_found() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let orphan = app
        .create_user(10017930, "No Membership", &["auth.profile.update"])
        .await;
    let member = app
        .create_user(10017931, "Has Membership", &["auth.profile.update"])
        .await;
    delete_membership(&app, &orphan.id).await;
    set_initials(&app, &member.id, "MB").await;

    let response = app
        .json_request(
            "PATCH",
            "/api/v1/me",
            Some(&orphan.session_token),
            Some(json!({ "operating_initials": "QR" })),
        )
        .await;
    assert_status(&response, StatusCode::NOT_FOUND);
    let body: Value = json_body(response).await;
    assert_eq!(body["error"], "not_found");
    assert_eq!(membership_count(&app, &orphan.id).await, 0);

    let response = app
        .json_request(
            "PATCH",
            "/api/v1/me",
            Some(&member.session_token),
            Some(json!({ "operating_initials": "QR" })),
        )
        .await;
    assert_status(&response, StatusCode::OK);
    assert_eq!(
        stored_initials(&app, &member.id).await.as_deref(),
        Some("QR")
    );

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn self_service_initials_taken_pair_is_conflict_and_changes_nothing() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let holder = app.create_user(10017940, "Holder", &[]).await;
    let member = app
        .create_user(10017941, "Has Membership", &["auth.profile.update"])
        .await;
    set_initials(&app, &holder.id, "AA").await;
    set_initials(&app, &member.id, "BB").await;

    let response = app
        .json_request(
            "PATCH",
            "/api/v1/me",
            Some(&member.session_token),
            Some(json!({ "operating_initials": "aa" })),
        )
        .await;
    assert_status(&response, StatusCode::CONFLICT);
    let body: Value = json_body(response).await;
    assert_eq!(body["error"], "conflict");
    assert_eq!(
        stored_initials(&app, &holder.id).await.as_deref(),
        Some("AA")
    );
    assert_eq!(
        stored_initials(&app, &member.id).await.as_deref(),
        Some("BB")
    );

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn admin_reassign_to_the_pair_already_held_succeeds() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app.create_user(10017950, "OI Staff", &[PERMISSION]).await;
    let target = app.create_user(10017951, "Target", &[]).await;
    set_initials(&app, &target.id, "BB").await;

    // Postgres counts a same-value update as an affected row, so re-sending
    // the held pair is a success, not a missing membership.
    let response = reassign(&app, &staff.session_token, target.cid, "BB").await;
    assert_status(&response, StatusCode::OK);
    let body: Value = json_body(response).await;
    assert_eq!(body["operating_initials"], "BB");
    assert_eq!(
        stored_initials(&app, &target.id).await.as_deref(),
        Some("BB")
    );
    assert_eq!(membership_count(&app, &target.id).await, 1);

    app.cleanup().await;
}
