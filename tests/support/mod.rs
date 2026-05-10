#![allow(dead_code)]

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use osmium::{
    email::service::EmailService,
    router,
    state::{AppState, EmailHealth, JobHealth},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

pub struct TestApp {
    pub pool: PgPool,
    pub app: Router,
    root_database_url: String,
    database_name: String,
    file_root: PathBuf,
    _env_guards: Vec<EnvVarGuard>,
}

pub struct TestUser {
    pub id: String,
    pub cid: i64,
    pub session_token: String,
}

pub fn env_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

impl TestApp {
    pub async fn new() -> Option<Self> {
        let root_database_url = std::env::var("DATABASE_URL").ok()?;
        let database_name = format!("osmium_test_{}", Uuid::new_v4().simple());
        let database_url = database_url_for_name(&root_database_url, &database_name);

        let root_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&root_database_url)
            .await
            .expect("connect root test database");

        sqlx::query(&format!("create database \"{database_name}\""))
            .execute(&root_pool)
            .await
            .expect("create isolated test database");

        root_pool.close().await;

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("connect isolated test database");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations for isolated test database");

        let file_root = std::env::temp_dir().join(format!("osmium-files-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&file_root).expect("create file storage root");

        let env_guards = vec![
            EnvVarGuard::set("FILE_STORAGE_ROOT", file_root.to_string_lossy().as_ref()),
            EnvVarGuard::set("FILE_SIGNING_SECRET", "test-signing-secret"),
            EnvVarGuard::set("CDN_BASE_URL", "http://127.0.0.1:3000"),
            EnvVarGuard::set("EMAIL_UNSUBSCRIBE_SECRET", "test-unsub-secret"),
            EnvVarGuard::set("EMAIL_UNSUBSCRIBE_BASE_URL", "http://127.0.0.1:3000"),
            EnvVarGuard::set("COOKIE_SECURE", "false"),
            EnvVarGuard::set("CORS_ALLOWED_ORIGINS", "http://127.0.0.1:3000"),
            EnvVarGuard::set("VATSIM_DEV_MODE", "false"),
            EnvVarGuard::set("DEV_LOGIN_AS_CID_ENABLED", "false"),
            EnvVarGuard::set("DEV_SEED_ENABLED", "false"),
        ];

        let email = Arc::new(EmailService::disabled());
        let (controller_events, _) = broadcast::channel(1024);
        let state = AppState {
            db: Some(pool.clone()),
            job_health: Arc::new(std::sync::RwLock::new(JobHealth::default())),
            email_health: Arc::new(std::sync::RwLock::new(EmailHealth::default())),
            email,
            controller_events,
        };

        let app = router::build_router(state);

        Some(Self {
            pool,
            app,
            root_database_url,
            database_name,
            file_root,
            _env_guards: env_guards,
        })
    }

    pub async fn cleanup(&self) {
        self.pool.close().await;
        let _ = std::fs::remove_dir_all(&self.file_root);

        let root_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.root_database_url)
            .await
            .expect("reconnect root test database");

        sqlx::query(
            "select pg_terminate_backend(pid) from pg_stat_activity where datname = $1 and pid <> pg_backend_pid()",
        )
        .bind(&self.database_name)
        .execute(&root_pool)
        .await
        .expect("terminate isolated test database connections");

        sqlx::query(&format!(
            "drop database if exists \"{}\"",
            self.database_name
        ))
        .execute(&root_pool)
        .await
        .expect("drop isolated test database");

        root_pool.close().await;
    }

    pub async fn create_user(
        &self,
        cid: i64,
        display_name: &str,
        permissions: &[&str],
    ) -> TestUser {
        let user_id = Uuid::new_v4().to_string();
        let actor_id = Uuid::new_v4().to_string();
        let session_token = Uuid::new_v4().to_string();
        let email = format!("user-{cid}@example.invalid");

        sqlx::query(
            r#"
            insert into identity.users (id, cid, email, full_name, display_name)
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&user_id)
        .bind(cid)
        .bind(&email)
        .bind(display_name)
        .bind(display_name)
        .execute(&self.pool)
        .await
        .expect("insert test user");

        sqlx::query(
            r#"
            insert into identity.user_profiles (user_id, timezone)
            values ($1, 'America/Chicago')
            "#,
        )
        .bind(&user_id)
        .execute(&self.pool)
        .await
        .expect("insert test user profile");

        sqlx::query(
            r#"
            insert into org.memberships (user_id, rating, operating_initials)
            values ($1, 'S1', $2)
            "#,
        )
        .bind(&user_id)
        .bind(format!("T{}", cid % 10))
        .execute(&self.pool)
        .await
        .expect("insert test membership");

        sqlx::query(
            r#"
            insert into access.actors (id, actor_type, user_id, display_name)
            values ($1, 'user', $2, $3)
            "#,
        )
        .bind(&actor_id)
        .bind(&user_id)
        .bind(display_name)
        .execute(&self.pool)
        .await
        .expect("insert test user actor");

        for permission in permissions {
            sqlx::query(
                r#"
                insert into access.user_permissions (user_id, permission_name, granted)
                values ($1, $2, true)
                on conflict (user_id, permission_name) do update
                set granted = true
                "#,
            )
            .bind(&user_id)
            .bind(*permission)
            .execute(&self.pool)
            .await
            .expect("grant test permission");
        }

        sqlx::query(
            r#"
            insert into identity.sessions (session_token, user_id, expires_at)
            values ($1, $2, now() + interval '30 days')
            "#,
        )
        .bind(&session_token)
        .bind(&user_id)
        .execute(&self.pool)
        .await
        .expect("insert test session");

        TestUser {
            id: user_id,
            cid,
            session_token,
        }
    }

    pub async fn request(&self, request: Request<Body>) -> Response<Body> {
        self.app
            .clone()
            .oneshot(request)
            .await
            .expect("execute test request")
    }

    pub async fn json_request(
        &self,
        method: &str,
        uri: &str,
        session_token: Option<&str>,
        body: Option<Value>,
    ) -> Response<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(token) = session_token {
            builder = builder.header(header::COOKIE, session_cookie(token));
        }

        let request = if let Some(body) = body {
            builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        };

        self.request(request).await
    }

    pub async fn raw_request(
        &self,
        method: &str,
        uri: &str,
        session_token: Option<&str>,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> Response<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(token) = session_token {
            builder = builder.header(header::COOKIE, session_cookie(token));
        }
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }

        self.request(builder.body(Body::from(body)).unwrap()).await
    }

    pub async fn bearer_request(
        &self,
        method: &str,
        uri: &str,
        bearer_token: &str,
    ) -> Response<Body> {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {bearer_token}"))
            .body(Body::empty())
            .unwrap();

        self.request(request).await
    }
}

pub async fn json_body<T: DeserializeOwned>(response: Response<Body>) -> T {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    serde_json::from_slice(&body).expect("parse response json")
}

pub async fn text_body(response: Response<Body>) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    String::from_utf8(body.to_vec()).expect("decode response body")
}

pub fn assert_status(response: &Response<Body>, expected: StatusCode) {
    assert_eq!(response.status(), expected);
}

fn session_cookie(token: &str) -> String {
    format!("osmium_session={token}")
}

fn database_url_for_name(root_url: &str, database_name: &str) -> String {
    let mut url = reqwest::Url::parse(root_url).expect("parse DATABASE_URL");
    url.set_path(&format!("/{database_name}"));
    url.to_string()
}

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
