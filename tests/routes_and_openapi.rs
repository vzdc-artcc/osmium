use std::sync::{Mutex, OnceLock};

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use tower::ServiceExt;

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }

        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_deref() {
            unsafe {
                std::env::set_var(self.key, previous);
            }
        } else {
            unsafe {
                std::env::remove_var(self.key);
            }
        }
    }
}

fn env_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    env_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tokio::test]
async fn health_endpoint_works_without_db() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();
    assert!(body_text.contains("ok"));
}

#[tokio::test]
async fn ready_endpoint_includes_job_health() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();
    assert!(body_text.contains("stats_sync"));
    assert!(body_text.contains("roster_sync"));
    assert!(body_text.contains("email_worker"));
    assert!(body_text.contains("docs"));
}

#[tokio::test]
async fn docs_index_renders() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();
    assert!(body_text.contains("Osmium Docs"));
    assert!(body_text.contains("Interactive API reference"));
}

#[tokio::test]
async fn all_registered_docs_pages_render() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    for page in osmium::docs::DOC_PAGES
        .iter()
        .filter(|page| !page.section.is_empty())
    {
        let uri = format!("/docs/{}/{}", page.section, page.slug);
        let response = app
            .clone()
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "failed docs page: {uri}");
    }
}

#[tokio::test]
async fn openapi_json_route_renders_and_covers_core_groups() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/docs/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let paths = json
        .get("paths")
        .and_then(|value| value.as_object())
        .unwrap();

    for expected in [
        "/api/v1/me",
        "/api/v1/me/teamspeak-uids",
        "/api/v1/me/teamspeak-uids/{identity_id}",
        "/api/v1/auth/service-account/me",
        "/api/v1/emails/templates",
        "/api/v1/emails/preview",
        "/api/v1/emails/send",
        "/api/v1/emails/outbox",
        "/api/v1/emails/outbox/{id}",
        "/api/v1/emails/preferences",
        "/api/v1/emails/resubscribe",
        "/api/v1/users",
        "/api/v1/users/visitor-application",
        "/api/v1/admin/acl",
        "/api/v1/admin/visitor-applications",
        "/api/v1/admin/visitor-applications/{application_id}",
        "/api/v1/training/assignments",
        "/api/v1/training/lessons",
        "/api/v1/training/lessons/{lesson_id}",
        "/api/v1/training/sessions",
        "/api/v1/training/sessions/{session_id}",
        "/api/v1/events",
        "/api/v1/feedback",
        "/api/v1/files",
        "/api/v1/publications",
        "/api/v1/publications/{publication_id}",
        "/api/v1/publications/categories",
        "/api/v1/admin/publications",
        "/api/v1/admin/publications/{publication_id}",
        "/api/v1/admin/publications/categories",
        "/api/v1/admin/publications/categories/{category_id}",
        "/api/v1/stats/artcc",
    ] {
        assert!(
            paths.contains_key(expected),
            "missing OpenAPI path: {expected}"
        );
    }
}

#[tokio::test]
async fn email_template_list_requires_auth() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/emails/templates")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn email_preferences_get_is_public_by_token() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/emails/preferences?token=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn email_preferences_post_is_public_by_token() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/emails/preferences")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"token":"invalid","preferences":[{"category":"announcements","subscribed":false}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn openapi_patch_me_schema_does_not_advertise_access_mutation_fields() {
    use utoipa::OpenApi;

    let openapi = serde_json::to_value(osmium::docs::ApiDoc::openapi()).unwrap();
    let patch_me = &openapi["components"]["schemas"]["PatchMeRequest"];

    assert!(patch_me["properties"]["permissions"].is_null());
    assert!(patch_me["properties"]["roles"].is_null());
    assert!(patch_me["properties"]["permission_overrides"].is_null());
    assert_eq!(
        patch_me["additionalProperties"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        patch_me["description"].as_str(),
        Some(
            "Self-service profile update payload. Only profile fields are accepted here. Roles, permissions, and access overrides must be changed through `POST /api/v1/admin/users/{cid}/access`."
        )
    );
}

#[test]
fn openapi_paginated_routes_use_envelopes_and_page_params() {
    use utoipa::OpenApi;

    let openapi = serde_json::to_value(osmium::docs::ApiDoc::openapi()).unwrap();

    for path in [
        "/api/v1/users",
        "/api/v1/events",
        "/api/v1/publications",
        "/api/v1/training/sessions",
        "/api/v1/admin/audit",
        "/api/v1/admin/loa",
    ] {
        let operation = &openapi["paths"][path]["get"];
        let parameters = operation["parameters"].as_array().unwrap();
        let parameter_names: Vec<_> = parameters
            .iter()
            .filter_map(|param| param["name"].as_str())
            .collect();

        assert!(
            parameter_names.contains(&"page"),
            "missing `page` for {path}"
        );
        assert!(
            parameter_names.contains(&"page_size"),
            "missing `page_size` for {path}"
        );
        assert!(
            parameter_names.contains(&"limit"),
            "missing legacy `limit` alias for {path}"
        );
        assert!(
            parameter_names.contains(&"offset"),
            "missing legacy `offset` alias for {path}"
        );

        let schema_ref =
            operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"]
                .as_str()
                .unwrap();
        let schema_name = schema_ref.rsplit('/').next().unwrap();
        let schema = &openapi["components"]["schemas"][schema_name];
        let properties = collect_schema_properties(&openapi, schema);

        for field in [
            "items",
            "total",
            "page",
            "page_size",
            "total_pages",
            "has_next",
            "has_prev",
        ] {
            assert!(
                properties.contains(&field.to_string()),
                "missing {field} for {path}"
            );
        }
    }
}

/// Flattened fields (`#[serde(flatten)]`) surface in utoipa's generated schema as
/// `allOf` composition over a `$ref`, not as direct sibling properties — this walks
/// that composition to collect the full flattened property set.
fn collect_schema_properties(
    openapi: &serde_json::Value,
    schema: &serde_json::Value,
) -> std::collections::HashSet<String> {
    let mut properties = std::collections::HashSet::new();

    if let Some(reference) = schema["$ref"].as_str() {
        let name = reference.rsplit('/').next().unwrap();
        properties.extend(collect_schema_properties(
            openapi,
            &openapi["components"]["schemas"][name],
        ));
        return properties;
    }

    if let Some(members) = schema["allOf"].as_array() {
        for member in members {
            properties.extend(collect_schema_properties(openapi, member));
        }
    }

    if let Some(object) = schema["properties"].as_object() {
        properties.extend(object.keys().cloned());
    }

    properties
}

#[tokio::test]
async fn swagger_ui_route_renders() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/docs/api/v1/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn dev_login_route_is_gated_by_dev_mode_env() {
    let _env_lock = lock_env();

    {
        let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");
        let _dev_login = EnvVarGuard::set("DEV_LOGIN_AS_CID_ENABLED", "false");

        let state = osmium::state::AppState::without_db();
        let app = osmium::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/login/as/10000010")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn logout_requires_post_method() {
    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let get_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let post_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(post_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dev_login_route_is_enabled_with_only_vatsim_dev_mode() {
    let _env_lock = lock_env();
    let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "true");
    let _dev_login = EnvVarGuard::set("DEV_LOGIN_AS_CID_ENABLED", "false");

    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/login/as/10000010")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dev_login_route_is_enabled_with_explicit_flag() {
    let _env_lock = lock_env();
    let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");
    let _dev_login = EnvVarGuard::set("DEV_LOGIN_AS_CID_ENABLED", "true");

    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/login/as/10000010")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn dev_seed_route_is_gated_by_dev_mode_env() {
    let _env_lock = lock_env();

    {
        let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");
        let _dev_seed = EnvVarGuard::set("DEV_SEED_ENABLED", "false");

        let state = osmium::state::AppState::without_db();
        let app = osmium::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/dev/seed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn dev_seed_route_is_not_enabled_by_vatsim_dev_mode() {
    let _env_lock = lock_env();
    let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "true");
    let _dev_seed = EnvVarGuard::set("DEV_SEED_ENABLED", "false");

    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dev/seed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dev_seed_route_is_enabled_with_explicit_flag() {
    let _env_lock = lock_env();
    let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");
    let _dev_seed = EnvVarGuard::set("DEV_SEED_ENABLED", "true");

    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dev/seed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn cors_preflight_reflects_allowed_origin_only() {
    let _env_lock = lock_env();
    let _cors = EnvVarGuard::set(
        "CORS_ALLOWED_ORIGINS",
        "http://127.0.0.1:3000,https://app.example.org",
    );

    let state = osmium::state::AppState::without_db();
    let app = osmium::router::build_router(state);

    let allowed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(http::header::ORIGIN, "https://app.example.org")
                .header(http::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(allowed.status(), StatusCode::OK);
    assert_eq!(
        allowed
            .headers()
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("https://app.example.org")
    );

    let blocked = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(http::header::ORIGIN, "https://blocked.example.org")
                .header(http::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(blocked.status(), StatusCode::OK);
    assert!(
        blocked
            .headers()
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}
