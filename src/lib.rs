pub mod auth;
pub mod docs;
pub mod errors;
pub mod handlers;
pub mod jobs;
pub mod logging;
pub mod models;
pub mod repos;
pub mod router;
pub mod state;

use std::net::SocketAddr;

use tracing_subscriber::{EnvFilter, fmt};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let state = state::AppState::from_env().await?;
    run_startup_migrations(&state).await?;
    jobs::stats_sync::start_stats_sync_worker(state.clone());
    jobs::roster_sync::start_roster_sync_worker(state.clone());

    let app = router::build_router(state);

    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()?;

    tracing::info!(%addr, "starting osmium api");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=debug".into());

    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}

fn startup_migrations_enabled() -> bool {
    std::env::var("RUN_MIGRATIONS_ON_STARTUP")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}

async fn run_startup_migrations(
    state: &state::AppState,
) -> Result<(), sqlx::migrate::MigrateError> {
    if !startup_migrations_enabled() {
        tracing::info!("startup migrations disabled");
        return Ok(());
    }

    let Some(pool) = state.db.as_ref() else {
        tracing::info!("startup migrations skipped (no database configured)");
        return Ok(());
    };

    tracing::info!("running startup migrations");
    let result = sqlx::migrate!("./migrations").run(pool).await;

    if let Err(sqlx::migrate::MigrateError::VersionMissing(version)) = &result {
        tracing::error!(
            %version,
            "database migration history contains an old version that no longer exists in this repo"
        );
        tracing::error!(
            "this usually means the dev database or Docker volume still has the pre-reset migration ledger"
        );
        tracing::error!(
            "compose recovery: `docker compose down -v && docker compose up -d postgres`"
        );
        tracing::error!(
            "manual recovery: drop and recreate the `osmium` database, then rerun the current 0001-0015 migration chain"
        );
    }

    result
}

#[cfg(test)]
mod tests {
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

    #[tokio::test]
    async fn health_endpoint_works_without_db() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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
        assert!(body_text.contains("docs"));
    }

    #[tokio::test]
    async fn docs_index_renders() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        for page in crate::docs::DOC_PAGES
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
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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
            "/api/v1/auth/service-account/me",
            "/api/v1/user",
            "/api/v1/user/visitor-application",
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
            "/api/v1/stats/artcc",
        ] {
            assert!(
                paths.contains_key(expected),
                "missing OpenAPI path: {expected}"
            );
        }
    }

    #[tokio::test]
    async fn swagger_ui_route_renders() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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
        let _env_lock = env_test_lock().lock().unwrap();

        {
            let _api_dev = EnvVarGuard::set("API_DEV_MODE", "false");
            let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");

            let state = crate::state::AppState::without_db();
            let app = crate::router::build_router(state);

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

        {
            let _api_dev = EnvVarGuard::set("API_DEV_MODE", "true");
            let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");

            let state = crate::state::AppState::without_db();
            let app = crate::router::build_router(state);

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
    }

    #[tokio::test]
    async fn logout_requires_post_method() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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

        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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
        let _env_lock = env_test_lock().lock().unwrap();
        let _api_dev = EnvVarGuard::set("API_DEV_MODE", "false");
        let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "true");

        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

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
        let _env_lock = env_test_lock().lock().unwrap();

        {
            let _api_dev = EnvVarGuard::set("API_DEV_MODE", "false");
            let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");

            let state = crate::state::AppState::without_db();
            let app = crate::router::build_router(state);

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

        {
            let _api_dev = EnvVarGuard::set("API_DEV_MODE", "true");
            let _vatsim_dev = EnvVarGuard::set("VATSIM_DEV_MODE", "false");

            let state = crate::state::AppState::without_db();
            let app = crate::router::build_router(state);

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
    }

    #[tokio::test]
    async fn admin_acl_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/acl")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_user_access_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/users/10000010/access")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_set_controller_status_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/admin/users/10000010/controller-status")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{\"controller_status\":\"VISITOR\"}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_list_visitor_applications_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/visitor-applications")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_decide_visitor_application_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/admin/visitor-applications/test-application-id")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{\"status\":\"APPROVED\"}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_access_catalog_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/access/catalog")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn user_list_endpoint_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/user?limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn user_lookup_endpoint_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/user/10000010")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn user_visit_artcc_endpoint_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/user/visit-artcc")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{\"artcc\":\"ZDC\"}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn user_get_visitor_application_endpoint_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/user/visitor-application")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn user_create_visitor_application_endpoint_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/user/visitor-application")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        "{\"home_facility\":\"ZNY\",\"why_visit\":\"Interested in events\"}",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn user_feedback_endpoint_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/user/10000010/feedback?limit=10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn artcc_stats_endpoint_is_public_but_requires_db() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/stats/artcc?month=3&year=2026")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn controller_history_stats_endpoint_is_public_but_requires_db() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/stats/controller/10000010/history?year=2026")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn controller_totals_stats_endpoint_is_public_but_requires_db() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/stats/controller/10000010/totals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn training_assignments_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/training/assignments")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_lessons_endpoint_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/training/lessons")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_lesson_create_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/training/lessons")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        "{\"identifier\":\"OBS1\",\"location\":0,\"name\":\"Intro\",\"description\":\"Desc\",\"position\":\"DCA_DEL\",\"facility\":\"DCA\",\"duration\":60,\"instructor_only\":false,\"notify_instructor_on_pass\":false,\"release_request_on_pass\":false}",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_lesson_update_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/training/lessons/lesson-1")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        "{\"identifier\":\"OBS1\",\"location\":0,\"name\":\"Intro\",\"description\":\"Desc\",\"position\":\"DCA_DEL\",\"facility\":\"DCA\",\"duration\":60,\"instructor_only\":false,\"notify_instructor_on_pass\":false,\"release_request_on_pass\":false}",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_lesson_delete_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/training/lessons/lesson-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_sessions_list_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/training/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_session_detail_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/training/sessions/session-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_session_create_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/training/sessions")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        "{\"student_id\":\"user-1\",\"start\":\"2026-04-28T12:00:00Z\",\"end\":\"2026-04-28T13:00:00Z\",\"tickets\":[]}",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_session_update_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/training/sessions/session-1")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        "{\"student_id\":\"user-1\",\"start\":\"2026-04-28T12:00:00Z\",\"end\":\"2026-04-28T13:00:00Z\",\"tickets\":[]}",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_session_delete_requires_staff_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/training/sessions/session-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_assignment_request_create_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/training/assignment-requests")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn training_assignment_request_interest_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/training/assignment-requests/request-1/interest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn feedback_create_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/feedback")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        "{\"target_cid\":10000010,\"pilot_callsign\":\"N123AB\",\"controller_position\":\"DCA_TWR\",\"rating\":4}",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn feedback_list_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/feedback")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn feedback_decide_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/feedback/fb-1")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{\"status\":\"RELEASED\"}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn event_position_create_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/events/event-1/positions")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        "{\"callsign\":\"DCA_DEL\",\"requested_slot\":1}",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn files_list_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/files")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn files_download_requires_db_when_not_configured() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/files/file-1/content")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn files_signed_url_requires_session() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/files/file-1/signed-url")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cdn_route_requires_db_when_not_configured() {
        let state = crate::state::AppState::without_db();
        let app = crate::router::build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/cdn/file-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
