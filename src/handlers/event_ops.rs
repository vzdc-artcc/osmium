use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        context::CurrentUser, permissions::EventsItemsUpdate, require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{
        CreateEventTmiRequest, EventOpsPlanItem, EventTmiItem, EventTmiListResponse,
        PaginationMeta, PaginationQuery, UpdateEventOpsPlanRequest,
        UpdateEventTmiRequest, UpdatePresetPositionsRequest,
    },
    repos::{audit as audit_repo, events as events_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

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
        (status = 404, description = "Event not found")
    )
)]
pub async fn get_event_ops_plan(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EventOpsPlanItem>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let plan = events_repo::fetch_event_ops_plan(pool, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(plan, time))
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
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Event not found")
    )
)]
pub async fn update_event_ops_plan(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<EventsItemsUpdate>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateEventOpsPlanRequest>,
) -> Result<ApiJson<EventOpsPlanItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = events_repo::fetch_event_ops_plan(pool, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = events_repo::update_event_ops_plan_row(
        pool,
        &event_id,
        payload.featured_fields,
        payload.preset_positions,
        payload.featured_field_configs,
        payload.tmis.is_some(),
        payload.tmis.flatten(),
        payload.ops_free_text.is_some(),
        payload.ops_free_text.flatten(),
        payload.ops_plan_published,
        payload.ops_planner_id.is_some(),
        payload.ops_planner_id.flatten(),
        payload.enable_buffer_times,
    )
    .await?
    .ok_or(ApiError::NotFound)?;
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
    Ok(ApiJson::new(row, time))
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
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EventTmiListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = events_repo::count_event_tmis(pool, &event_id).await?;
    let rows =
        events_repo::list_event_tmis(pool, &event_id, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        EventTmiListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
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
    _permission: RequirePermission<EventsItemsUpdate>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateEventTmiRequest>,
) -> Result<(StatusCode, ApiJson<EventTmiItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.tmi_type.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = events_repo::insert_event_tmi(
        pool,
        &Uuid::new_v4().to_string(),
        &event_id,
        payload.tmi_type.trim(),
        payload.start_time,
        payload
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .await?;
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
    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
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
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Event or TMI not found")
    )
)]
pub async fn update_event_tmi(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<EventsItemsUpdate>,
    Path((event_id, tmi_id)): Path<(String, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateEventTmiRequest>,
) -> Result<ApiJson<EventTmiItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = events_repo::fetch_event_tmi(pool, &event_id, &tmi_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = events_repo::update_event_tmi_row(
        pool,
        &event_id,
        &tmi_id,
        payload
            .tmi_type
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload.start_time,
        payload.notes.is_some(),
        payload.notes.flatten(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
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
    Ok(ApiJson::new(row, time))
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
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Event or TMI not found")
    )
)]
pub async fn delete_event_tmi(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<EventsItemsUpdate>,
    Path((event_id, tmi_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = events_repo::fetch_event_tmi(pool, &event_id, &tmi_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    events_repo::delete_event_tmi_row(pool, &event_id, &tmi_id).await?;
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
        (status = 404, description = "Event not found")
    )
)]
pub async fn get_event_preset_positions(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let positions = events_repo::fetch_preset_positions(pool, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;
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
    _permission: RequirePermission<EventsItemsUpdate>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePresetPositionsRequest>,
) -> Result<Json<Vec<String>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    events_repo::update_preset_positions_row(pool, &event_id, &payload.preset_positions).await?;
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
    _permission: RequirePermission<EventsItemsUpdate>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    events_repo::set_positions_locked(pool, &event_id, true).await?;
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
    _permission: RequirePermission<EventsItemsUpdate>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    events_repo::set_positions_locked(pool, &event_id, false).await?;
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

async fn record_audit(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    headers: &HeaderMap,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    before_state: Option<serde_json::Value>,
    after_state: Option<serde_json::Value>,
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
