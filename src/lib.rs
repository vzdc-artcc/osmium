pub mod auth;
pub mod errors;
pub mod handlers;
pub mod models;
pub mod router;
pub mod state;

use std::net::SocketAddr;

use tracing_subscriber::{EnvFilter, fmt};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let state = state::AppState::from_env().await?;
    run_startup_migrations(&state).await?;

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
    sqlx::migrate!("./migrations").run(pool).await
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
}
