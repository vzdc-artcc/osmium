mod support;

use axum::http::StatusCode;
use serde_json::Value;
use support::{TestApp, assert_status, json_body, lock_env};

/// Runs the login-bootstrap path a real VATSIM OAuth callback runs — directly,
/// since the dev `login/as/{cid}` route that used to trigger it was retired when
/// authenticated impersonation shipped (spec 012). This exercises exactly the same
/// `bootstrap_login_user` + `ensure_user_login_access` logic these tests protect.
async fn login_as(app: &TestApp, cid: i64) {
    let email = format!("dev-cid-{cid}@example.invalid");
    let name = format!("Dev CID {cid}");
    let (user_id, was_new_user) =
        osmium::handlers::auth::bootstrap_login_user(&app.pool, cid, &email, &name, &name, None)
            .await
            .expect("bootstrap login user");
    osmium::handlers::auth::ensure_user_login_access(&app.pool, &user_id, cid, was_new_user)
        .await
        .expect("ensure user login access");
}

/// The core regression this migration exists to prevent: baseline
/// permissions must be seeded once, on the login that first creates the
/// identity.users row, and never touched again — so a permission an admin
/// grants later (via the staff permissions editor) survives the user's next
/// login instead of being silently wiped back to the baseline.
#[tokio::test(flavor = "current_thread")]
async fn baseline_seeded_once_and_admin_grants_survive_later_logins() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let cid = 10000300i64;

    login_as(&app, cid).await;

    let user_id: String = sqlx::query_scalar("select id from identity.users where cid = $1")
        .bind(cid)
        .fetch_one(&app.pool)
        .await
        .expect("user created on first login");

    let perms_after_first: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1 order by permission_name",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .expect("fetch permissions after first login");

    assert!(perms_after_first.contains(&"auth.profile.read".to_string()));
    assert!(perms_after_first.contains(&"feedback.items.create".to_string()));
    let baseline_count = perms_after_first.len();
    assert!(baseline_count > 0);

    // Simulate an admin granting an extra permission via the permissions editor.
    sqlx::query(
        "insert into access.user_permissions (user_id, permission_name, granted) values ($1, 'training.lessons.read', true)
         on conflict (user_id, permission_name) do update set granted = true",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .expect("simulate admin grant");

    // Second login for the same (now-existing) user must not touch
    // access.user_permissions at all.
    login_as(&app, cid).await;

    let perms_after_second: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1 order by permission_name",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .expect("fetch permissions after second login");

    assert_eq!(perms_after_second.len(), baseline_count + 1);
    assert!(perms_after_second.contains(&"training.lessons.read".to_string()));
    assert!(perms_after_second.contains(&"auth.profile.read".to_string()));

    app.cleanup().await;
}

/// OSMIUM_SERVER_ADMIN_CID sync is idempotent and must keep working
/// unconditionally on every login, independent of the was_new_user change.
#[tokio::test(flavor = "current_thread")]
async fn server_admin_cid_gets_role_on_every_login() {
    let _env_lock = lock_env();
    let cid = 10000301i64;
    let cid_string = cid.to_string();
    let Some(app) =
        TestApp::new_with_env_overrides(&[("OSMIUM_SERVER_ADMIN_CID", &cid_string)]).await
    else {
        return;
    };

    login_as(&app, cid).await;
    login_as(&app, cid).await;

    let user_id: String = sqlx::query_scalar("select id from identity.users where cid = $1")
        .bind(cid)
        .fetch_one(&app.pool)
        .await
        .expect("user created");

    let is_server_admin: bool = sqlx::query_scalar(
        "select exists(select 1 from access.user_roles where user_id = $1 and role_name = 'SERVER_ADMIN')",
    )
    .bind(&user_id)
    .fetch_one(&app.pool)
    .await
    .expect("check server admin role");

    assert!(is_server_admin);

    app.cleanup().await;
}

/// A user removed from OSMIUM_SERVER_ADMIN_CID must lose the SERVER_ADMIN role
/// on their next login — the sync reconciles demotions, not just promotions — so
/// a former admin cannot silently retain server-admin access. The demoted
/// account is left as an ordinary user with baseline self-service permissions
/// rather than locked out with none.
#[tokio::test(flavor = "current_thread")]
async fn server_admin_role_is_revoked_when_cid_no_longer_configured() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let cid = 10000304i64;
    let cid_string = cid.to_string();

    // First login while configured as a server admin.
    unsafe {
        std::env::set_var("OSMIUM_SERVER_ADMIN_CID", &cid_string);
    }
    login_as(&app, cid).await;

    let user_id: String = sqlx::query_scalar("select id from identity.users where cid = $1")
        .bind(cid)
        .fetch_one(&app.pool)
        .await
        .expect("user created");

    let is_admin_before: bool = sqlx::query_scalar(
        "select exists(select 1 from access.user_roles where user_id = $1 and role_name = 'SERVER_ADMIN')",
    )
    .bind(&user_id)
    .fetch_one(&app.pool)
    .await
    .expect("check role before demotion");
    assert!(is_admin_before, "should hold SERVER_ADMIN while configured");

    // CID removed from the env: the next login must revoke the role.
    unsafe {
        std::env::remove_var("OSMIUM_SERVER_ADMIN_CID");
    }
    login_as(&app, cid).await;

    let is_admin_after: bool = sqlx::query_scalar(
        "select exists(select 1 from access.user_roles where user_id = $1 and role_name = 'SERVER_ADMIN')",
    )
    .bind(&user_id)
    .fetch_one(&app.pool)
    .await
    .expect("check role after demotion");
    assert!(
        !is_admin_after,
        "SERVER_ADMIN must be revoked once the cid is no longer configured"
    );

    let perms: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .expect("fetch permissions after demotion");
    assert!(
        perms.contains(&"auth.profile.read".to_string()),
        "demoted user should be reset to baseline self-service access, not locked out"
    );

    app.cleanup().await;
}

/// The STAFF role must now carry access.catalog.read/access.users.read/
/// access.users.update (migration 0047) so the permissions editor is usable
/// by role membership alone, without also needing a direct grant.
#[tokio::test(flavor = "current_thread")]
async fn staff_role_can_use_access_management_endpoints() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let staff = app.create_user(10000302, "Access Staff", &[]).await;
    let target = app.create_user(10000303, "Access Target", &[]).await;

    sqlx::query(
        "insert into access.user_roles (user_id, role_name) values ($1, 'STAFF') on conflict do nothing",
    )
    .bind(&staff.id)
    .execute(&app.pool)
    .await
    .expect("grant STAFF role");

    let catalog_response = app
        .json_request(
            "GET",
            "/api/v1/admin/access/catalog",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&catalog_response, StatusCode::OK);

    let get_access_response = app
        .json_request(
            "GET",
            &format!("/api/v1/admin/users/{}/access", target.cid),
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&get_access_response, StatusCode::OK);
    let access_body: Value = json_body(get_access_response).await;
    assert_eq!(access_body["server_admin"], false);

    app.cleanup().await;
}
