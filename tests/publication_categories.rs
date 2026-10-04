mod support;

use axum::http::StatusCode;
use support::{TestApp, assert_status, lock_env};

async fn seed_category_with_file(app: &TestApp, uploader_id: &str) {
    sqlx::raw_sql(&format!(
        "insert into web.publication_categories (id, key, name) values ('cat-sops', 'sops', 'SOPs');
         insert into media.file_assets (id, filename, content_type, size_bytes, etag, storage_key,
                                        uploaded_by, domain_type, domain_id)
             values ('file-sop', 'sop.pdf', 'application/pdf', 1, 'etag', 'key/sop.pdf',
                     '{uploader_id}', 'publication', 'pub-sop');
         insert into web.publications (id, category_id, title, effective_at, file_id)
             values ('pub-sop', 'cat-sops', 'SOP', now(), 'file-sop');"
    ))
    .execute(&app.pool)
    .await
    .expect("seed category, file and publication");
}

async fn count(app: &TestApp, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&app.pool).await.unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn deleting_a_category_with_files_needs_both_permissions_and_deletes_the_files() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let category_only = app
        .create_user(
            10000178,
            "Category Only",
            &["publications.categories.delete"],
        )
        .await;
    let both = app
        .create_user(
            10000179,
            "Category And Items",
            &[
                "publications.categories.delete",
                "publications.items.delete",
            ],
        )
        .await;
    seed_category_with_file(&app, &both.id).await;
    let path = "/api/v1/admin/publications/categories/cat-sops";

    let response = app
        .json_request("DELETE", path, Some(&category_only.session_token), None)
        .await;
    assert!(
        matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ),
        "expected a refusal, got {}",
        response.status()
    );
    assert_eq!(
        count(
            &app,
            "select count(*) from web.publications where id = 'pub-sop'"
        )
        .await,
        1,
        "a refused delete removes nothing"
    );

    let response = app
        .json_request("DELETE", path, Some(&both.session_token), None)
        .await;
    assert_status(&response, StatusCode::NO_CONTENT);
    assert_eq!(
        count(
            &app,
            "select count(*) from web.publication_categories where id = 'cat-sops'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &app,
            "select count(*) from web.publications where id = 'pub-sop'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &app,
            "select count(*) from media.file_assets where id = 'file-sop' and domain_id is null"
        )
        .await,
        1,
        "the file asset is detached, as a single delete does"
    );
    assert_eq!(
        count(
            &app,
            "select count(*) from access.audit_logs
             where action = 'DELETE' and resource_type in ('PUBLICATION', 'PUBLICATION_CATEGORY')"
        )
        .await,
        2
    );

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn an_empty_category_needs_only_the_category_permission() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };
    let staff = app
        .create_user(
            10000180,
            "Category Only",
            &["publications.categories.delete"],
        )
        .await;
    sqlx::raw_sql(
        "insert into web.publication_categories (id, key, name) values ('cat-empty', 'empty', 'Empty');",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let response = app
        .json_request(
            "DELETE",
            "/api/v1/admin/publications/categories/cat-empty",
            Some(&staff.session_token),
            None,
        )
        .await;
    assert_status(&response, StatusCode::NO_CONTENT);

    app.cleanup().await;
}
