pub mod auth;
pub mod captcha;
pub mod config;
pub mod docs;
pub mod email;
pub mod errors;
pub mod handlers;
pub mod jobs;
pub mod logging;
pub mod models;
pub mod rate_limit;
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
    sync_email_templates(&state).await;
    jobs::email_delivery::start_email_delivery_worker(state.clone());
    jobs::stats_sync::start_stats_sync_worker(state.clone());
    jobs::roster_sync::start_roster_sync_worker(state.clone());
    jobs::event_lifecycle::start_event_lifecycle_worker(state.clone());
    jobs::loa_expiration::start_loa_expiration_worker(state.clone());
    jobs::solo_expiration::start_solo_expiration_worker(state.clone());
    jobs::appointments_sync::start_appointments_sync_worker(state.clone());
    jobs::faa_preferred_routes::start_faa_preferred_routes_worker(state.clone());
    jobs::ip_log_writer::start_ip_log_writer_worker(state.clone());
    jobs::ip_log_cleanup::start_ip_log_cleanup_worker(state.clone());

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

/// Mirror the code template registry into `email.templates` so every enqueueable
/// template satisfies the `email.outbox.template_id` FK. Best-effort: logged, not
/// fatal — but a failure means template sends (e.g. roster-sync progression mail)
/// will keep hitting the FK until it succeeds.
async fn sync_email_templates(state: &state::AppState) {
    let Some(pool) = state.db.as_ref() else {
        return;
    };
    match state.email.sync_template_registry(pool).await {
        Ok(count) => tracing::info!(templates = count, "synced email template registry"),
        Err(error) => {
            tracing::error!(?error, "failed to sync email template registry")
        }
    }
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
