use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthBody {
    status: &'static str,
}

pub async fn health() -> Json<HealthBody> {
    Json(HealthBody { status: "ok" })
}

pub async fn ready(State(state): State<AppState>) -> Json<HealthBody> {
    if let Some(pool) = state.db {
        if sqlx::query_scalar::<_, i32>("select 1")
            .fetch_one(&pool)
            .await
            .is_ok()
        {
            return Json(HealthBody { status: "ready" });
        }
    }

    Json(HealthBody { status: "degraded" })
}
