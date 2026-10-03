//! The permission catalog must never contain a path that is a prefix of another.
//!
//! Clients receive permissions as a nested tree (`acl::permission_tree_from_paths`)
//! where a node is either a leaf action array or a parent of further segments,
//! never both. A colliding pair loses one permission from the tree, and the
//! access editor posts that tree back as the user's full grant set.

mod support;

use std::collections::BTreeSet;

use osmium::auth::acl::PermissionPath;
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
