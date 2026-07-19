pub mod auth;
pub mod config;
pub mod docs;
pub mod email;
pub mod errors;
pub mod handlers;
pub mod jobs;
pub mod logging;
pub mod models;
pub mod repos;
pub mod router;
pub mod state;
pub mod time;

use std::net::SocketAddr;

use tracing_subscriber::{EnvFilter, fmt};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let state = state::AppState::from_env().await?;
    run_startup_migrations(&state).await?;
    jobs::email_delivery::start_email_delivery_worker(state.clone());
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
