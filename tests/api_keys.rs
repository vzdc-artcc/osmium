mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use support::{TestApp, assert_status, json_body, lock_env};

#[tokio::test(flavor = "current_thread")]
async fn service_account_with_integrations_stats_update_can_list_discord_configs() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let creator = app
        .create_user(
            10000031,
            "Discord Bot Key Creator",
            &["api_keys.create", "integrations.stats.update"],
        )
        .await;

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/api-keys",
            Some(&creator.session_token),
            Some(json!({
                "name": "Discord Bot Key",
                "permissions": {
                    "integrations": {
                        "stats": ["update"]
                    }
                }
            })),
        )
        .await;
    assert_status(&create_response, StatusCode::CREATED);
    let create_body: Value = json_body(create_response).await;
    let secret = create_body["secret"].as_str().unwrap().to_string();

    sqlx::query(
        r#"
        insert into integration.discord_configs (id, name, guild_id)
        values ('cfg-1', 'Primary', 'guild-1')
        "#,
    )
    .execute(&app.pool)
    .await
    .expect("insert discord config");
    sqlx::query(
        r#"
        insert into integration.discord_channels (id, discord_config_id, name, channel_id)
        values ('chan-1', 'cfg-1', 'ops', 'discord-chan-1')
        "#,
    )
    .execute(&app.pool)
    .await
    .expect("insert discord channel");
    sqlx::query(
        r#"
        insert into integration.discord_roles (id, discord_config_id, name, role_id)
        values ('role-1', 'cfg-1', 'controllers', 'discord-role-1')
        "#,
    )
    .execute(&app.pool)
    .await
    .expect("insert discord role");
    sqlx::query(
        r#"
        insert into integration.discord_categories (id, discord_config_id, name, category_id)
        values ('cat-1', 'cfg-1', 'briefings', 'discord-cat-1')
        "#,
    )
    .execute(&app.pool)
    .await
    .expect("insert discord category");

    let bearer_response = app
        .bearer_request("GET", "/api/v1/admin/integrations/discord/configs", &secret)
        .await;
    assert_status(&bearer_response, StatusCode::OK);
    let bearer_body: Value = json_body(bearer_response).await;
    assert_eq!(bearer_body["configs"][0]["id"], "cfg-1");
    assert_eq!(bearer_body["channels"][0]["id"], "chan-1");
    assert_eq!(bearer_body["roles"][0]["id"], "role-1");
    assert_eq!(bearer_body["categories"][0]["id"], "cat-1");

    let session_response = app
        .json_request(
            "GET",
            "/api/v1/admin/integrations/discord/configs",
            Some(&creator.session_token),
            None,
        )
        .await;
    assert_status(&session_response, StatusCode::OK);

    let create_attempt = app
        .request(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/integrations/discord/configs")
                .header(header::AUTHORIZATION, format!("Bearer {secret}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Blocked Config",
                        "guild_id": "guild-2"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_status(&create_attempt, StatusCode::UNAUTHORIZED);

    let update_attempt = app
        .request(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/admin/integrations/discord/configs/cfg-1")
                .header(header::AUTHORIZATION, format!("Bearer {secret}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": "Blocked Update"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_status(&update_attempt, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn service_account_without_integrations_stats_update_cannot_list_discord_configs() {
    let _env_lock = lock_env();
    let Some(app) = TestApp::new().await else {
        return;
    };

    let creator = app
        .create_user(
            10000032,
            "Limited Key Creator",
            &["api_keys.create", "auth.profile.read"],
        )
        .await;

    let create_response = app
        .json_request(
            "POST",
            "/api/v1/api-keys",
            Some(&creator.session_token),
            Some(json!({
                "name": "Limited Key",
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
    let secret = create_body["secret"].as_str().unwrap().to_string();

    let bearer_response = app
        .bearer_request("GET", "/api/v1/admin/integrations/discord/configs", &secret)
        .await;
    assert_status(&bearer_response, StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test(flavor = "current_thread")]
async fn api_key_lifecycle_works_end_to_end() {
    let _env_lock = lock_env();
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
