use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{ListEventsQuery, PaginationMeta, PaginationQuery},
    repos::audit as audit_repo,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct EventOpsPlanItem {
    pub id: String,
    pub title: String,
    pub positions_locked: bool,
    pub manual_positions_open: bool,
    pub featured_fields: Vec<String>,
    pub preset_positions: Vec<String>,
    pub featured_field_configs: Option<Value>,
    pub tmis: Option<String>,
    pub ops_free_text: Option<String>,
    pub ops_plan_published: bool,
    pub ops_planner_id: Option<String>,
    pub enable_buffer_times: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct EventTmiItem {
    pub id: String,
    pub event_id: String,
    pub tmi_type: String,
    pub start_time: DateTime<Utc>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventTmiListResponse {
    pub items: Vec<EventTmiItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEventOpsPlanRequest {
    pub featured_fields: Option<Vec<String>>,
    pub preset_positions: Option<Vec<String>>,
    pub featured_field_configs: Option<Value>,
    pub tmis: Option<Option<String>>,
    pub ops_free_text: Option<Option<String>>,
    pub ops_plan_published: Option<bool>,
    pub ops_planner_id: Option<Option<String>>,
    pub enable_buffer_times: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEventTmiRequest {
    pub tmi_type: String,
    pub start_time: DateTime<Utc>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEventTmiRequest {
    pub tmi_type: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub notes: Option<Option<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePresetPositionsRequest {
    pub preset_positions: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/events/{event_id}/ops-plan",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "Event ops plan", body = EventOpsPlanItem),
        (status = 400, description = "Invalid event ID")
    )
)]
pub async fn get_event_ops_plan(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<EventOpsPlanItem>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(Json(fetch_event_ops_plan(pool, &event_id).await?))
}

#[utoipa::path(
    patch,
    path = "/api/v1/events/{event_id}/ops-plan",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    request_body = UpdateEventOpsPlanRequest,
    responses(
        (status = 200, description = "Updated event ops plan", body = EventOpsPlanItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn update_event_ops_plan(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateEventOpsPlanRequest>,
) -> Result<Json<EventOpsPlanItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_event_update(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_event_ops_plan(pool, &event_id).await?;
    let row = sqlx::query_as::<_, EventOpsPlanItem>(
        r#"
        update events.events
        set featured_fields = coalesce($2, featured_fields),
            preset_positions = coalesce($3, preset_positions),
            featured_field_configs = coalesce($4, featured_field_configs),
            tmis = case when $5::bool then $6 else tmis end,
            ops_free_text = case when $7::bool then $8 else ops_free_text end,
            ops_plan_published = coalesce($9, ops_plan_published),
            ops_planner_id = case when $10::bool then $11 else ops_planner_id end,
            enable_buffer_times = coalesce($12, enable_buffer_times),
            updated_at = now()
        where id = $1
        returning id, title, positions_locked, manual_positions_open, featured_fields, preset_positions, featured_field_configs, tmis, ops_free_text, ops_plan_published, ops_planner_id, enable_buffer_times, updated_at
        "#,
    )
    .bind(&event_id)
    .bind(payload.featured_fields)
    .bind(payload.preset_positions)
    .bind(payload.featured_field_configs)
    .bind(payload.tmis.is_some())
    .bind(payload.tmis.flatten())
    .bind(payload.ops_free_text.is_some())
    .bind(payload.ops_free_text.flatten())
    .bind(payload.ops_plan_published)
    .bind(payload.ops_planner_id.is_some())
    .bind(payload.ops_planner_id.flatten())
    .bind(payload.enable_buffer_times)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "EVENT_OPS_PLAN",
        Some(event_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok(Json(row))
}

#[utoipa::path(
    get,
    path = "/api/v1/events/{event_id}/tmis",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID"),
        PaginationQuery
    ),
    responses(
        (status = 200, description = "Event TMI list", body = EventTmiListResponse),
        (status = 400, description = "Invalid event ID")
    )
)]
pub async fn list_event_tmis(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<EventTmiListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from events.event_tmis where event_id = $1",
    )
    .bind(&event_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    let rows = sqlx::query_as::<_, EventTmiItem>(
        "select id, event_id, tmi_type, start_time, notes, created_at, updated_at from events.event_tmis where event_id = $1 order by start_time asc, created_at asc, id asc limit $2 offset $3",
    )
    .bind(&event_id)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(EventTmiListResponse {
        items: rows,
        total: meta.total,
        page: meta.page,
        page_size: meta.page_size,
        total_pages: meta.total_pages,
        has_next: meta.has_next,
        has_prev: meta.has_prev,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/events/{event_id}/tmis",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    request_body = CreateEventTmiRequest,
    responses(
        (status = 201, description = "Event TMI created", body = EventTmiItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_event_tmi(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<CreateEventTmiRequest>,
) -> Result<(StatusCode, Json<EventTmiItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_event_update(&state, user).await?;
    if payload.tmi_type.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = sqlx::query_as::<_, EventTmiItem>(
        "insert into events.event_tmis (id, event_id, tmi_type, start_time, notes, created_at, updated_at) values ($1, $2, $3, $4, $5, now(), now()) returning id, event_id, tmi_type, start_time, notes, created_at, updated_at",
    ).bind(Uuid::new_v4().to_string()).bind(&event_id).bind(payload.tmi_type.trim()).bind(payload.start_time).bind(payload.notes.as_deref().map(str::trim).filter(|v| !v.is_empty())).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "EVENT_TMI",
        Some(row.id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/events/{event_id}/tmis/{tmi_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID"),
        ("tmi_id" = String, Path, description = "Event TMI ID")
    ),
    request_body = UpdateEventTmiRequest,
    responses(
        (status = 200, description = "Updated event TMI", body = EventTmiItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn update_event_tmi(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path((event_id, tmi_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<UpdateEventTmiRequest>,
) -> Result<Json<EventTmiItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_event_update(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_event_tmi(pool, &event_id, &tmi_id).await?;
    let row = sqlx::query_as::<_, EventTmiItem>(
        r#"
        update events.event_tmis
        set tmi_type = coalesce($3, tmi_type),
            start_time = coalesce($4, start_time),
            notes = case when $5::bool then $6 else notes end,
            updated_at = now()
        where event_id = $1 and id = $2
        returning id, event_id, tmi_type, start_time, notes, created_at, updated_at
        "#,
    )
    .bind(&event_id)
    .bind(&tmi_id)
    .bind(
        payload
            .tmi_type
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .bind(payload.start_time)
    .bind(payload.notes.is_some())
    .bind(payload.notes.flatten())
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "EVENT_TMI",
        Some(tmi_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok(Json(row))
}

#[utoipa::path(
    delete,
    path = "/api/v1/events/{event_id}/tmis/{tmi_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID"),
        ("tmi_id" = String, Path, description = "Event TMI ID")
    ),
    responses(
        (status = 200, description = "Deleted event TMI", body = ApiMessageBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn delete_event_tmi(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path((event_id, tmi_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_event_update(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_event_tmi(pool, &event_id, &tmi_id).await?;
    sqlx::query("delete from events.event_tmis where event_id = $1 and id = $2")
        .bind(&event_id)
        .bind(&tmi_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    record_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "EVENT_TMI",
        Some(tmi_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "event tmi deleted".to_string(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/events/{event_id}/preset-positions",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "Preset positions", body = [String]),
        (status = 400, description = "Invalid event ID")
    )
)]
pub async fn get_event_preset_positions(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let positions = sqlx::query_scalar::<_, Vec<String>>(
        "select preset_positions from events.events where id = $1",
    )
    .bind(&event_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;
    Ok(Json(positions))
}

#[utoipa::path(
    put,
    path = "/api/v1/events/{event_id}/preset-positions",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    request_body = UpdatePresetPositionsRequest,
    responses(
        (status = 200, description = "Updated preset positions", body = [String]),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn update_event_preset_positions(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePresetPositionsRequest>,
) -> Result<Json<Vec<String>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_event_update(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    sqlx::query("update events.events set preset_positions = $2, updated_at = now() where id = $1")
        .bind(&event_id)
        .bind(&payload.preset_positions)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "EVENT_PRESET_POSITIONS",
        Some(event_id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&payload.preset_positions)?),
    )
    .await?;
    Ok(Json(payload.preset_positions))
}

#[utoipa::path(
    post,
    path = "/api/v1/events/{event_id}/positions/lock",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "Locked event positions", body = ApiMessageBody),
        (status = 400, description = "Invalid event ID"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn lock_event_positions(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_event_update(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    sqlx::query(
        "update events.events set positions_locked = true, updated_at = now() where id = $1",
    )
    .bind(&event_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "EVENT_POSITION_LOCK",
        Some(event_id.clone()),
        None,
        Some(serde_json::json!({ "positions_locked": true })),
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "event positions locked".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/events/{event_id}/positions/unlock",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "Unlocked event positions", body = ApiMessageBody),
        (status = 400, description = "Invalid event ID"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn unlock_event_positions(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_event_update(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    sqlx::query(
        "update events.events set positions_locked = false, updated_at = now() where id = $1",
    )
    .bind(&event_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "EVENT_POSITION_LOCK",
        Some(event_id.clone()),
        None,
        Some(serde_json::json!({ "positions_locked": false })),
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "event positions unlocked".to_string(),
    }))
}

async fn ensure_event_update(state: &AppState, user: &CurrentUser) -> Result<(), ApiError> {
    ensure_permission(
        state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "items"], PermissionAction::Update),
    )
    .await
}

async fn fetch_event_ops_plan(
    pool: &sqlx::PgPool,
    event_id: &str,
) -> Result<EventOpsPlanItem, ApiError> {
    sqlx::query_as::<_, EventOpsPlanItem>(
        "select id, title, positions_locked, manual_positions_open, featured_fields, preset_positions, featured_field_configs, tmis, ops_free_text, ops_plan_published, ops_planner_id, enable_buffer_times, updated_at from events.events where id = $1",
    ).bind(event_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)
}

async fn fetch_event_tmi(
    pool: &sqlx::PgPool,
    event_id: &str,
    tmi_id: &str,
) -> Result<EventTmiItem, ApiError> {
    sqlx::query_as::<_, EventTmiItem>(
        "select id, event_id, tmi_type, start_time, notes, created_at, updated_at from events.event_tmis where event_id = $1 and id = $2",
    ).bind(event_id).bind(tmi_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)
}

async fn record_audit(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    headers: &HeaderMap,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    before_state: Option<Value>,
    after_state: Option<Value>,
) -> Result<(), ApiError> {
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            scope_type: "event".to_string(),
            scope_key: Some(user.cid.to_string()),
            before_state,
            after_state,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}
