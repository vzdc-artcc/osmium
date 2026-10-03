//! The permission catalog must never contain a path that is a prefix of another.
//!
//! Clients receive permissions as a nested tree (`acl::permission_tree_from_paths`)
//! where a node is either a leaf action array or a parent of further segments,
//! never both. A colliding pair loses one permission from the tree, and the
//! access editor posts that tree back as the user's full grant set.

mod support;

use std::collections::BTreeSet;

use osmium::auth::acl::PermissionPath;
use sqlx::postgres::PgPoolOptions;
use support::{TestApp, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn no_permission_path_is_a_prefix_of_another() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let names: Vec<String> = sqlx::query_scalar("select name from access.permissions")
        .fetch_all(&app.pool)
        .await
        .expect("load permission catalog");
    assert!(!names.is_empty(), "permission catalog is empty");

    let paths: BTreeSet<Vec<String>> = names
        .iter()
        .map(|name| {
            PermissionPath::from_db_value(name)
                .unwrap_or_else(|| panic!("unparseable permission name {name}"))
                .segments
        })
        .collect();

    let collisions: Vec<String> = paths
        .iter()
        .flat_map(|parent| {
            paths
                .iter()
                .filter(move |child| child.len() > parent.len() && child.starts_with(parent))
                .map(move |child| {
                    format!("{} is a prefix of {}", parent.join("."), child.join("."))
                })
        })
        .collect();

    assert!(
        collisions.is_empty(),
        "colliding permission paths:\n{}",
        collisions.join("\n")
    );

    app.cleanup().await;
}

/// Grants held under the pre-0072 names (role, user grant, user revoke, and
/// service account) must all move to the renamed permissions. The grant tables
/// cascade on delete, so a missed table would silently drop those grants.
#[tokio::test(flavor = "current_thread")]
async fn migration_0072_moves_every_grant_to_the_renamed_permission() {
    let _env_lock = lock_env();
    let Ok(root_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let database_name = format!("osmium_test_{}", uuid::Uuid::new_v4().simple());
    let mut database_url = reqwest::Url::parse(&root_url).expect("parse DATABASE_URL");
    database_url.set_path(&format!("/{database_name}"));

    let root = PgPoolOptions::new()
        .max_connections(1)
        .connect(&root_url)
        .await
        .expect("connect root database");
    sqlx::query(&format!("create database \"{database_name}\""))
        .execute(&root)
        .await
        .expect("create database");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url.as_str())
        .await
        .expect("connect test database");

    let migrator = sqlx::migrate!("./migrations");
    let apply = |version: i64| {
        let migration = migrator
            .iter()
            .find(|migration| migration.version == version)
            .expect("migration exists");
        let pool = pool.clone();
        let sql = migration.sql.to_string();
        async move {
            let mut tx = pool.begin().await.expect("begin");
            sqlx::raw_sql(&sql)
                .execute(&mut *tx)
                .await
                .expect("apply migration");
            tx.commit().await.expect("commit");
        }
    };
    for version in migrator.iter().map(|m| m.version).filter(|v| *v < 72) {
        apply(version).await;
    }

    sqlx::raw_sql(
        "insert into identity.users (id, cid, full_name, display_name) values ('u-0072', 7200001, 'Rename Target', 'Rename Target');
         insert into access.service_accounts (id, key, name) values ('sa-0072', 'sa-0072', 'Rename Key');
         insert into access.roles (name) values ('ROLE_0072');
         insert into access.role_permissions (role_name, permission_name) values ('ROLE_0072', 'training.assignment_requests.self.request');
         insert into access.user_permissions (user_id, permission_name, granted) values
             ('u-0072', 'events.positions.self.request', true),
             ('u-0072', 'files.assets.policy.update', false);
         insert into access.service_account_permissions (service_account_id, permission_name, granted) values
             ('sa-0072', 'users.visitor_applications.self.read', true);",
    )
    .execute(&pool)
    .await
    .expect("seed grants under the old names");

    apply(72).await;

    let role: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.role_permissions where role_name = 'ROLE_0072'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(role, vec!["training.assignment_requests_self.request"]);

    let mut user: Vec<(String, bool)> = sqlx::query_as(
        "select permission_name, granted from access.user_permissions where user_id = 'u-0072'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    user.sort();
    assert_eq!(
        user,
        vec![
            ("events.positions_self.request".to_string(), true),
            ("files.assets_policy.update".to_string(), false),
        ]
    );

    let service_account: Vec<String> = sqlx::query_scalar(
        "select permission_name from access.service_account_permissions where service_account_id = 'sa-0072'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        service_account,
        vec!["users.visitor_applications_self.read"]
    );

    let old_left: i64 = sqlx::query_scalar(
        "select count(*) from access.permissions where name in (
            'training.assignment_requests.self.request', 'events.positions.self.request',
            'files.assets.policy.update', 'users.visitor_applications.self.read')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(old_left, 0);

    pool.close().await;
    sqlx::query(&format!(
        "drop database if exists \"{database_name}\" with (force)"
    ))
    .execute(&root)
    .await
    .expect("drop test database");
}
