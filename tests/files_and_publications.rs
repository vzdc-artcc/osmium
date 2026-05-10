mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env, text_body};

#[tokio::test(flavor = "current_thread")]
async fn file_upload_download_and_cdn_visibility_work() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let user = app
        .create_user(
            10000031,
            "File User",
            &[
                "files.assets.create",
                "files.content.create",
                "files.assets.read",
                "files.content.read",
            ],
        )
        .await;

    let upload_response = app
        .raw_request(
            "POST",
            "/api/v1/files?filename=test.txt&public=true",
            Some(&user.session_token),
            Some("text/plain"),
            b"hello from integration test".to_vec(),
        )
        .await;
    assert_status(&upload_response, StatusCode::CREATED);
    let upload_body: Value = json_body(upload_response).await;
    let file_id = upload_body["id"].as_str().unwrap().to_string();

    let download_response = app
        .json_request(
            "GET",
            &format!("/api/v1/files/{file_id}/content"),
            Some(&user.session_token),
            None,
        )
        .await;
    assert_status(&download_response, StatusCode::OK);
    assert_eq!(
        text_body(download_response).await,
        "hello from integration test"
    );

    let signed_url_response = app
        .json_request(
            "GET",
            &format!("/api/v1/files/{file_id}/signed-url"),
            Some(&user.session_token),
            None,
        )
        .await;
    assert_status(&signed_url_response, StatusCode::OK);
    let signed_url_body: Value = json_body(signed_url_response).await;
    let signed_url = signed_url_body["url"].as_str().unwrap();
    let signed_path = signed_url
        .strip_prefix("http://127.0.0.1:3000")
        .unwrap()
        .to_string();

    let cdn_response = app.json_request("GET", &signed_path, None, None).await;
    assert_status(&cdn_response, StatusCode::OK);
    assert_eq!(text_body(cdn_response).await, "hello from integration test");

    let private_upload_response = app
        .raw_request(
            "POST",
            "/api/v1/files?filename=private.txt&public=false",
            Some(&user.session_token),
            Some("text/plain"),
            b"private bytes".to_vec(),
        )
        .await;
    assert_status(&private_upload_response, StatusCode::CREATED);
    let private_upload_body: Value = json_body(private_upload_response).await;
    let private_file_id = private_upload_body["id"].as_str().unwrap();

    let anonymous_private_response = app
        .json_request("GET", &format!("/cdn/{private_file_id}"), None, None)
        .await;
    assert_status(&anonymous_private_response, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn publication_visibility_rules_hold_with_real_db_state() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let admin = app
        .create_user(
            10000032,
            "Publication Admin",
            &[
                "files.assets.create",
                "files.content.create",
                "files.assets.read",
                "files.content.read",
                "publications.categories.create",
                "publications.categories.read",
                "publications.items.create",
                "publications.items.read",
            ],
        )
        .await;

    let category_response = app
        .json_request(
            "POST",
            "/api/v1/admin/publications/categories",
            Some(&admin.session_token),
            Some(json!({
                "key": "integration-category",
                "name": "Integration Category",
                "description": "created during integration test",
                "sort_order": 99
            })),
        )
        .await;
    assert_status(&category_response, StatusCode::CREATED);
    let category_body: Value = json_body(category_response).await;
    let category_id = category_body["id"].as_str().unwrap().to_string();

    let public_file_response = app
        .raw_request(
            "POST",
            "/api/v1/files?filename=public.pdf&public=true",
            Some(&admin.session_token),
            Some("application/pdf"),
            b"public pdf bytes".to_vec(),
        )
        .await;
    assert_status(&public_file_response, StatusCode::CREATED);
    let public_file_body: Value = json_body(public_file_response).await;
    let public_file_id = public_file_body["id"].as_str().unwrap().to_string();

    let draft_file_response = app
        .raw_request(
            "POST",
            "/api/v1/files?filename=draft.pdf&public=false",
            Some(&admin.session_token),
            Some("application/pdf"),
            b"draft pdf bytes".to_vec(),
        )
        .await;
    assert_status(&draft_file_response, StatusCode::CREATED);
    let draft_file_body: Value = json_body(draft_file_response).await;
    let draft_file_id = draft_file_body["id"].as_str().unwrap().to_string();

    let effective_at = "2026-05-09T12:00:00Z";
    let published_response = app
        .json_request(
            "POST",
            "/api/v1/admin/publications",
            Some(&admin.session_token),
            Some(json!({
                "category_id": category_id,
                "title": "Published Item",
                "description": "visible to the public",
                "effective_at": effective_at,
                "file_id": public_file_id,
                "is_public": true,
                "sort_order": 1,
                "status": "published"
            })),
        )
        .await;
    assert_status(&published_response, StatusCode::CREATED);
    let published_body: Value = json_body(published_response).await;
    let published_id = published_body["id"].as_str().unwrap().to_string();

    let draft_response = app
        .json_request(
            "POST",
            "/api/v1/admin/publications",
            Some(&admin.session_token),
            Some(json!({
                "category_id": category_id,
                "title": "Draft Item",
                "description": "not visible to the public",
                "effective_at": effective_at,
                "file_id": draft_file_id,
                "is_public": false,
                "sort_order": 2,
                "status": "draft"
            })),
        )
        .await;
    assert_status(&draft_response, StatusCode::CREATED);
    let draft_body: Value = json_body(draft_response).await;
    let draft_id = draft_body["id"].as_str().unwrap().to_string();

    let public_list_response = app
        .json_request("GET", "/api/v1/publications", None, None)
        .await;
    assert_status(&public_list_response, StatusCode::OK);
    let public_list_body: Value = json_body(public_list_response).await;
    assert_eq!(public_list_body["items"].as_array().unwrap().len(), 1);
    assert_eq!(public_list_body["items"][0]["id"], published_id);

    let public_detail_response = app
        .json_request(
            "GET",
            &format!("/api/v1/publications/{published_id}"),
            None,
            None,
        )
        .await;
    assert_status(&public_detail_response, StatusCode::OK);

    let hidden_detail_response = app
        .json_request(
            "GET",
            &format!("/api/v1/publications/{draft_id}"),
            None,
            None,
        )
        .await;
    assert_status(&hidden_detail_response, StatusCode::BAD_REQUEST);

    let admin_detail_response = app
        .json_request(
            "GET",
            &format!("/api/v1/admin/publications/{draft_id}"),
            Some(&admin.session_token),
            None,
        )
        .await;
    assert_status(&admin_detail_response, StatusCode::OK);

    app.cleanup().await;
}
