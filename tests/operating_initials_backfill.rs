mod support;

use support::{TestApp, lock_env};

/// osmium#90: a legacy-migration row can carry `controller_status = 'HOME'`
/// or `'VISITOR'` (already on the active roster) without ever having gone
/// through either of the two triggers that call `ensure_operating_initials`
/// — a real osmium login, or an admin controller-status change — so it sits
/// with no operating initials indefinitely. This exercises the startup
/// backfill directly against a row shaped exactly like that gap, the same
/// way `tests/auth_login_bootstrap.rs` exercises `bootstrap_login_user`
/// without a real HTTP login.
#[tokio::test(flavor = "current_thread")]
async fn backfill_assigns_initials_to_an_active_member_with_none() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let cid = 10000400i64;
    let user_id: String = sqlx::query_scalar(
        "insert into identity.users (id, cid, email, full_name, display_name, first_name, last_name)
         values (gen_random_uuid()::text, $1, $2, $3, $3, $4, $5)
         returning id",
    )
    .bind(cid)
    .bind(format!("migrated-cid-{cid}@example.invalid"))
    .bind("Migrated Controller")
    .bind("Migrated")
    .bind("Controller")
    .fetch_one(&app.pool)
    .await
    .expect("insert legacy-shaped user");

    sqlx::query(
        "insert into org.memberships (user_id, rating, controller_status, operating_initials)
         values ($1, 'S1', 'HOME', null)",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .expect("insert legacy-shaped HOME membership with no initials");

    osmium::backfill_operating_initials(app.state.clone()).await;

    let initials: Option<String> =
        sqlx::query_scalar("select operating_initials from org.memberships where user_id = $1")
            .bind(&user_id)
            .fetch_one(&app.pool)
            .await
            .expect("fetch operating_initials after backfill");

    assert!(
        initials.is_some_and(|value| value.len() == 2),
        "an active HOME member with no operating initials must be backfilled"
    );

    app.cleanup().await;
}

/// A member who is `NONE` (never activated, or purged off the roster) must
/// stay without initials — the backfill is scoped to the active roster, not
/// every row with a gap.
#[tokio::test(flavor = "current_thread")]
async fn backfill_leaves_a_none_status_member_untouched() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let cid = 10000401i64;
    let user_id: String = sqlx::query_scalar(
        "insert into identity.users (id, cid, email, full_name, display_name, first_name, last_name)
         values (gen_random_uuid()::text, $1, $2, $3, $3, $4, $5)
         returning id",
    )
    .bind(cid)
    .bind(format!("none-cid-{cid}@example.invalid"))
    .bind("None Status Controller")
    .bind("None")
    .bind("Status")
    .fetch_one(&app.pool)
    .await
    .expect("insert user");

    sqlx::query(
        "insert into org.memberships (user_id, rating, controller_status, operating_initials)
         values ($1, 'S1', 'NONE', null)",
    )
    .bind(&user_id)
    .execute(&app.pool)
    .await
    .expect("insert NONE-status membership with no initials");

    osmium::backfill_operating_initials(app.state.clone()).await;

    let initials: Option<String> =
        sqlx::query_scalar("select operating_initials from org.memberships where user_id = $1")
            .bind(&user_id)
            .fetch_one(&app.pool)
            .await
            .expect("fetch operating_initials after backfill");

    assert!(
        initials.is_none(),
        "a NONE-status member must not be assigned operating initials"
    );

    app.cleanup().await;
}
