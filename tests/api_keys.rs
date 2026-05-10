mod support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use support::{TestApp, assert_status, env_test_lock, json_body};

#[tokio::test(flavor = "current_thread")]
async fn api_key_lifecycle_works_end_to_end() {
    let _env_lock = env_test_lock().lock().unwrap();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let creator = app
        .create_user(
            10000021,
            "API Key Creator",
            &["api_keys.create", "auth.profile.read"],
        )
        .await;

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/api-keys",
            Some(&creator.session_token),
            Some(json!({
                "name": "Integration Key",
                "description": "created in integration test",
                "permissions": {
                    "auth": {
                        "profile": ["read"]
                    }
                }
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);

    let create_body: Value = json_body(create_response).await;
    let key_id = create_body["key"]["id"].as_str().unwrap().to_string();
    let secret = create_body["secret"].as_str().unwrap().to_string();
    assert!(secret.starts_with("osm_"));
    assert!(create_body["key"].get("secret").is_none());

    let list_response = app
        .json_request(
            "GET",
            "/api/v1/api-keys",
            Some(&creator.session_token),
            None,
        )
        .await;
    assert_status(&list_response, StatusCode::OK);
    let list_body: Value = json_body(list_response).await;
    assert_eq!(list_body["items"][0]["id"], key_id);
    assert!(list_body["items"][0].get("secret").is_none());

    let detail_response = app
        .json_request(
            "GET",
            &format!("/api/v1/api-keys/{key_id}"),
            Some(&creator.session_token),
            None,
        )
        .await;
    assert_status(&detail_response, StatusCode::OK);
    let detail_body: Value = json_body(detail_response).await;
    assert_eq!(detail_body["permissions"]["auth"]["profile"][0], "read");

    let update_response = app
        .json_request(
            "PATCH",
            &format!("/api/v1/api-keys/{key_id}"),
            Some(&creator.session_token),
            Some(json!({
                "name": "Integration Key Updated",
                "description": "updated in integration test"
            })),
        )
        .await;
    assert_status(&update_response, StatusCode::OK);
    let update_body: Value = json_body(update_response).await;
    assert_eq!(update_body["name"], "Integration Key Updated");

    let bearer_response = app
        .bearer_request("GET", "/api/v1/auth/service-account/me", &secret)
        .await;
    assert_status(&bearer_response, StatusCode::OK);
    let bearer_body: Value = json_body(bearer_response).await;
    assert_eq!(bearer_body["permissions"]["auth"]["profile"][0], "read");

    let revoke_response = app
        .json_request(
            "DELETE",
            &format!("/api/v1/api-keys/{key_id}"),
            Some(&creator.session_token),
            None,
        )
        .await;
    assert_status(&revoke_response, StatusCode::NO_CONTENT);

    let revoked_bearer_response = app
        .bearer_request("GET", "/api/v1/auth/service-account/me", &secret)
        .await;
    assert_status(&revoked_bearer_response, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}
