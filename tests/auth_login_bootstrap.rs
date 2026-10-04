mod support;

use axum::http::StatusCode;
use serde_json::Value;
use support::{EnvVarGuard, TestApp, assert_status, json_body, lock_env};

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

/// Baseline permissions are seeded once, on the login that first creates the
/// identity.users row, and never touched again, so a permission an admin
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

/// A row created ahead of a real login (the shape a legacy-data migration
/// leaves behind) makes `was_new_user` false on that user's first real login,
/// since the identity.users insert already happened — the login only ever
/// takes the ON CONFLICT path. Such an account must still get the baseline,
/// or every self-service action stays 403 forever (osmium#87).
#[tokio::test(flavor = "current_thread")]
async fn baseline_is_seeded_for_a_pre_existing_user_with_zero_permissions() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let cid = 10000305i64;

    let user_id: String = sqlx::query_scalar(
        "insert into identity.users (id, cid, email, full_name, display_name)
         values (gen_random_uuid()::text, $1, $2, $3, $3)
         returning id",
    )
    .bind(cid)
    .bind(format!("migrated-cid-{cid}@example.invalid"))
    .bind(format!("Migrated CID {cid}"))
    .fetch_one(&app.pool)
    .await
    .expect("simulate a pre-existing migrated user row");

    let perms_before: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .expect("fetch permissions before login");
    assert!(
        perms_before.is_empty(),
        "row must start with no permissions"
    );

    // Real login path: upsert_login_user takes the ON CONFLICT branch since the
    // row already exists, so was_new_user comes back false here.
    login_as(&app, cid).await;

    let perms_after: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1 order by permission_name",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .expect("fetch permissions after login");

    assert!(
        perms_after.contains(&"auth.profile.read".to_string()),
        "a pre-existing account with no permissions must be seeded to the baseline on login"
    );
    assert!(perms_after.contains(&"feedback.items.create".to_string()));

    app.cleanup().await;
}

/// A pre-existing account holding only an explicit admin revoke (`granted =
/// false`, no `granted = true` rows at all) must keep that revoke on login —
/// seeding the rest of the baseline must never flip it back to `true`.
#[tokio::test(flavor = "current_thread")]
async fn login_does_not_flip_an_explicit_deny_row_back_to_granted() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let cid = 10000306i64;

    let user_id: String = sqlx::query_scalar(
        "insert into identity.users (id, cid, email, full_name, display_name)
         values (gen_random_uuid()::text, $1, $2, $3, $3)
         returning id",
    )
    .bind(cid)
    .bind(format!("migrated-cid-{cid}@example.invalid"))
    .bind(format!("Migrated CID {cid}"))
    .fetch_one(&app.pool)
    .await
    .expect("simulate a pre-existing migrated user row");

    sqlx::query(
        "insert into access.user_permissions (user_id, permission_name, granted)
         values ($1, 'auth.profile.update', false)",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .expect("simulate an explicit admin revoke");

    login_as(&app, cid).await;

    let granted: bool = sqlx::query_scalar(
        "select granted from access.user_permissions where user_id = $1 and permission_name = 'auth.profile.update'",
    )
    .bind(&user_id)
    .fetch_one(&app.pool)
    .await
    .expect("fetch the revoked row after login");
    assert!(!granted, "an explicit admin revoke must survive login");

    let perms_after: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1 and granted = true",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .expect("fetch granted permissions after login");
    assert!(
        perms_after.contains(&"feedback.items.create".to_string()),
        "the rest of the baseline must still be topped up"
    );

    app.cleanup().await;
}

/// An unseeded account an admin partially provisions before it logs in (one
/// baseline permission granted, the rest missing) must still get the rest of
/// the baseline when it is seeded at login — the same 403 wall osmium#87 reports,
/// reached through a different door than a currently-zero-permission account.
#[tokio::test(flavor = "current_thread")]
async fn login_tops_up_an_account_an_admin_partially_provisioned_before_first_login() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let cid = 10000307i64;

    let user_id: String = sqlx::query_scalar(
        "insert into identity.users (id, cid, email, full_name, display_name)
         values (gen_random_uuid()::text, $1, $2, $3, $3)
         returning id",
    )
    .bind(cid)
    .bind(format!("migrated-cid-{cid}@example.invalid"))
    .bind(format!("Migrated CID {cid}"))
    .fetch_one(&app.pool)
    .await
    .expect("simulate a pre-existing migrated user row");

    sqlx::query(
        "insert into access.user_permissions (user_id, permission_name, granted)
         values ($1, 'auth.profile.read', true)",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .expect("simulate an admin grant made before the account's first login");

    login_as(&app, cid).await;

    let perms_after: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1 and granted = true order by permission_name",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .expect("fetch granted permissions after login");

    assert!(perms_after.contains(&"auth.profile.read".to_string()));
    assert!(
        perms_after.contains(&"feedback.items.create".to_string()),
        "an account with a single pre-existing grant must still get the rest of the baseline on its first login"
    );

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

    // First login while configured as a server admin. The scoped guard restores
    // OSMIUM_SERVER_ADMIN_CID to its prior state on drop so the mutation never
    // leaks to later tests.
    {
        let _admin = EnvVarGuard::set("OSMIUM_SERVER_ADMIN_CID", &cid_string);
        login_as(&app, cid).await;
    }

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

    // CID removed from the env: the next login must revoke the role. Scoped so
    // the unset is likewise restored on drop.
    {
        let _demote = EnvVarGuard::unset("OSMIUM_SERVER_ADMIN_CID");
        login_as(&app, cid).await;
    }

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

/// Builds the editor's `{resource: {sub: [actions]}}` tree from dotted names.
fn permission_tree(names: &[&str]) -> Value {
    let mut tree = serde_json::Map::new();
    for name in names {
        let mut parts: Vec<&str> = name.split('.').collect();
        let action = parts.pop().unwrap();
        let mut node = &mut tree;
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                let leaf = node
                    .entry(part.to_string())
                    .or_insert_with(|| Value::Array(vec![]));
                leaf.as_array_mut().unwrap().push(Value::from(action));
            } else {
                node = node
                    .entry(part.to_string())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()))
                    .as_object_mut()
                    .unwrap();
            }
        }
    }
    Value::Object(tree)
}

/// Seeding happens once. A baseline permission an admin removes through the
/// permissions editor must not come back at the user's next login.
#[tokio::test(flavor = "current_thread")]
async fn login_does_not_restore_a_baseline_permission_the_editor_removed() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let baseline = osmium::handlers::auth::BASELINE_SELF_SERVICE_PERMISSIONS;
    let mut actor_permissions = baseline.to_vec();
    actor_permissions.push("access.users.update");
    let admin = app
        .create_user(10000310, "Access Admin", &actor_permissions)
        .await;

    let cid = 10000311i64;
    login_as(&app, cid).await;

    let removed = "events.positions.self.request";
    let kept: Vec<&str> = baseline.iter().copied().filter(|p| *p != removed).collect();
    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{cid}/access"),
            Some(&admin.session_token),
            Some(serde_json::json!({
                "permissions": permission_tree(&kept),
                "reason": "event signups suspended"
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    login_as(&app, cid).await;

    let still_held: i64 = sqlx::query_scalar(
        "select count(*) from access.user_permissions up join identity.users u on u.id = up.user_id
         where u.cid = $1 and up.permission_name = $2 and up.granted",
    )
    .bind(cid)
    .bind(removed)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        still_held, 0,
        "the editor's revoke must survive the next login"
    );

    app.cleanup().await;
}

/// Migration 0078 must grant every baseline permission, under either naming of
/// the `.self` permissions.
#[test]
fn backfill_migration_lists_every_baseline_permission() {
    let migration = include_str!("../migrations/0078_baseline_seeded_marker.sql");
    for permission in osmium::handlers::auth::BASELINE_SELF_SERVICE_PERMISSIONS {
        assert!(
            migration.contains(&format!("('{permission}')")),
            "0078 is missing baseline permission {permission}"
        );
    }
}

/// The backfill seeds an unmarked account once, keeps an explicit deny, marks
/// the account, and does nothing for an account already marked.
#[tokio::test(flavor = "current_thread")]
async fn backfill_migration_seeds_unmarked_accounts_once() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let migration = include_str!("../migrations/0078_baseline_seeded_marker.sql");

    // An account shaped like a migrated one before 0078: no marker, one deny.
    let user_id: String = sqlx::query_scalar(
        "insert into identity.users (id, cid, email, full_name, display_name)
         values (gen_random_uuid()::text, 10000312, 'm@example.invalid', 'Migrated', 'Migrated')
         returning id",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into access.user_permissions (user_id, permission_name, granted)
         values ($1, 'auth.profile.update', false)",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    sqlx::raw_sql(migration).execute(&app.pool).await.unwrap();

    let granted: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1 and granted",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert!(granted.contains(&"auth.profile.read".to_string()));
    assert!(
        !granted.contains(&"auth.profile.update".to_string()),
        "the explicit deny survives the backfill"
    );
    let marked: bool = sqlx::query_scalar(
        "select baseline_seeded_at is not null from identity.users where id = $1",
    )
    .bind(&user_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(marked);

    // Once marked, running the backfill again does not restore a removed grant.
    sqlx::query(
        "delete from access.user_permissions where user_id = $1 and permission_name = 'auth.profile.read'",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::raw_sql(migration).execute(&app.pool).await.unwrap();
    let restored: i64 = sqlx::query_scalar(
        "select count(*) from access.user_permissions where user_id = $1 and permission_name = 'auth.profile.read'",
    )
    .bind(&user_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(restored, 0);

    app.cleanup().await;
}

/// What a never-seeded account actually hits: `/me` (the profile pages' session
/// check) needs `auth.profile.read`, and creating its own ATC booking needs
/// `auth.profile.update`; listing bookings needs only a session. Seeding at login
/// clears both refusals.
#[tokio::test(flavor = "current_thread")]
async fn a_never_seeded_account_is_refused_profile_and_own_booking_until_seeded() {
    let _env_lock = lock_env();
    let _token = EnvVarGuard::set("ATC_BOOKING_TOKEN", "");
    let Some(app) = TestApp::new().await else {
        return;
    };
    let user = app.create_user(10000313, "Never Seeded", &[]).await;
    let booking = serde_json::json!({
        "callsign": "DCA_GND", "cid": user.cid,
        "start": "2030-01-01 12:00:00", "end": "2030-01-01 13:00:00"
    });
    let refused =
        |status: StatusCode| matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN);

    let me = app
        .json_request("GET", "/api/v1/me", Some(&user.session_token), None)
        .await;
    assert!(refused(me.status()), "/me before seeding: {}", me.status());
    let list = app
        .json_request("GET", "/api/v1/bookings", Some(&user.session_token), None)
        .await;
    assert_status(&list, StatusCode::SERVICE_UNAVAILABLE);
    let create = app
        .json_request(
            "POST",
            "/api/v1/bookings",
            Some(&user.session_token),
            Some(booking.clone()),
        )
        .await;
    assert!(
        refused(create.status()),
        "own booking before seeding: {}",
        create.status()
    );

    login_as(&app, user.cid).await;

    let me = app
        .json_request("GET", "/api/v1/me", Some(&user.session_token), None)
        .await;
    assert_status(&me, StatusCode::OK);
    let create = app
        .json_request(
            "POST",
            "/api/v1/bookings",
            Some(&user.session_token),
            Some(booking),
        )
        .await;
    assert_status(&create, StatusCode::SERVICE_UNAVAILABLE);

    app.cleanup().await;
}

/// An editor save on an account that has never been seeded (a migrator import
/// after 0078) seeds it first and applies only the admin's change on top, so a
/// revoke made before the account's first login survives that login, and the
/// rest of the baseline is still granted.
#[tokio::test(flavor = "current_thread")]
async fn an_editor_revoke_before_first_login_survives_that_login() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let baseline = osmium::handlers::auth::BASELINE_SELF_SERVICE_PERMISSIONS;
    let mut actor_permissions = baseline.to_vec();
    actor_permissions.push("access.users.update");
    let admin = app
        .create_user(10000314, "Access Admin", &actor_permissions)
        .await;

    // Imported after 0078: unmarked, partially provisioned.
    let cid = 10000315i64;
    let user_id: String = sqlx::query_scalar(
        "insert into identity.users (id, cid, email, full_name, display_name)
         values (gen_random_uuid()::text, $1, 'late@example.invalid', 'Late Import', 'Late Import')
         returning id",
    )
    .bind(cid)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into access.user_permissions (user_id, permission_name, granted)
         values ($1, 'events.positions.self.request', true)",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    // The admin sees only that one grant and swaps it for profile read.
    let response = app
        .json_request(
            "POST",
            &format!("/api/v1/admin/users/{cid}/access"),
            Some(&admin.session_token),
            Some(serde_json::json!({
                "permissions": permission_tree(&["auth.profile.read"]),
                "reason": "no event signups"
            })),
        )
        .await;
    assert_status(&response, StatusCode::OK);

    login_as(&app, cid).await;

    let granted: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.user_permissions where user_id = $1 and granted",
    )
    .bind(&user_id)
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert!(
        !granted.contains(&"events.positions.self.request".to_string()),
        "the revoke made before first login survives it"
    );
    assert!(
        granted.contains(&"feedback.items.create".to_string()),
        "the rest of the baseline is still granted"
    );

    app.cleanup().await;
}
