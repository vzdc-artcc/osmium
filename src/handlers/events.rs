use axum::{
    Json,
    extract::Extension,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionKey, PermissionResource},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{
        AssignEventPositionRequest, CreateEventPositionRequest, CreateEventRequest, Event,
        EventPosition, UpdateEventRequest,
    },
    state::AppState,
};

// List events
#[utoipa::path(
    get,
    path = "/api/v1/events",
    tag = "events",
    responses(
        (status = 200, description = "List events", body = [Event])
    )
)]
pub async fn list_events(State(state): State<AppState>) -> Result<Json<Vec<Event>>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let events = sqlx::query_as::<_, Event>(
        "SELECT id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at FROM events.events ORDER BY starts_at DESC"
    )
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(events))
}

// Get single event
#[utoipa::path(
    get,
    path = "/api/v1/events/{event_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "Event details", body = Event),
        (status = 400, description = "Invalid event ID")
    )
)]
pub async fn get_event(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<Event>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let event = sqlx::query_as::<_, Event>(
        "SELECT id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at FROM events.events WHERE id = $1"
    )
    .bind(&event_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    Ok(Json(event))
}

// Create event (staff only)
#[utoipa::path(
    post,
    path = "/api/v1/events",
    tag = "events",
    request_body = CreateEventRequest,
    responses(
        (status = 201, description = "Event created", body = Event),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_event(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(req): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<Event>), ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    )
    .await?;

    let event_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let event = sqlx::query_as::<_, Event>(
        "INSERT INTO events.events (id, title, type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at)
         VALUES ($1, $2, COALESCE($3, 'STANDARD'), $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at"
    )
    .bind(&event_id)
    .bind(&req.title)
    .bind(&req.event_type)
    .bind(&req.host)
    .bind(&req.description)
    .bind("SCHEDULED")
    .bind(false)
    .bind(&req.starts_at)
    .bind(&req.ends_at)
    .bind(&user.id)
    .bind(&now)
    .bind(&now)
    .fetch_one(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(event)))
}

// Update event (staff only)
#[utoipa::path(
    patch,
    path = "/api/v1/events/{event_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    request_body = UpdateEventRequest,
    responses(
        (status = 200, description = "Event updated", body = Event),
        (status = 400, description = "Invalid event ID"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_event(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    Json(req): Json<UpdateEventRequest>,
) -> Result<Json<Event>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    )
    .await?;
    let now = chrono::Utc::now();

    let event = sqlx::query_as::<_, Event>(
        "UPDATE events.events SET
            title = COALESCE($1, title),
            type = COALESCE($2, type),
            host = COALESCE($3, host),
            description = COALESCE($4, description),
            status = COALESCE($5, status),
            published = COALESCE($6, published),
            starts_at = COALESCE($7, starts_at),
            ends_at = COALESCE($8, ends_at),
            updated_at = $9
         WHERE id = $10
         RETURNING id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at"
    )
    .bind(req.title)
    .bind(req.event_type)
    .bind(req.host)
    .bind(req.description)
    .bind(req.status)
    .bind(req.published)
    .bind(req.starts_at)
    .bind(req.ends_at)
    .bind(&now)
    .bind(&event_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    Ok(Json(event))
}

// Delete event (staff only)
#[utoipa::path(
    delete,
    path = "/api/v1/events/{event_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 204, description = "Event deleted"),
        (status = 400, description = "Invalid event ID"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn delete_event(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    )
    .await?;

    let result = sqlx::query("DELETE FROM events.events WHERE id = $1")
        .bind(&event_id)
        .execute(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest);
    }

    Ok(StatusCode::NO_CONTENT)
}

// List event positions
#[utoipa::path(
    get,
    path = "/api/v1/events/{event_id}/positions",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "List event positions", body = [EventPosition]),
        (status = 400, description = "Invalid event ID")
    )
)]
pub async fn list_event_positions(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<Vec<EventPosition>>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let positions = sqlx::query_as::<_, EventPosition>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE event_id = $1 ORDER BY assigned_slot ASC NULLS LAST"
    )
    .bind(&event_id)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(positions))
}

// Create event position (user signup)
#[utoipa::path(
    post,
    path = "/api/v1/events/{event_id}/positions",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    request_body = CreateEventPositionRequest,
    responses(
        (status = 201, description = "Position request created", body = EventPosition),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn create_event_position(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    Json(req): Json<CreateEventPositionRequest>,
) -> Result<(StatusCode, Json<EventPosition>), ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let position_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let position = sqlx::query_as::<_, EventPosition>(
        "INSERT INTO events.event_positions (id, event_id, callsign, requested_slot, status, published, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at"
    )
    .bind(&position_id)
    .bind(&event_id)
    .bind(&req.callsign)
    .bind(&req.requested_slot)
    .bind("REQUESTED")
    .bind(false)
    .bind(&now)
    .bind(&now)
    .fetch_one(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(position)))
}

// Assign event position (staff only)
#[utoipa::path(
    patch,
    path = "/api/v1/events/{event_id}/positions/{position_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID"),
        ("position_id" = String, Path, description = "Position ID")
    ),
    request_body = AssignEventPositionRequest,
    responses(
        (status = 200, description = "Position assigned", body = EventPosition),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn assign_event_position(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path((event_id, position_id)): Path<(String, String)>,
    Json(req): Json<AssignEventPositionRequest>,
) -> Result<Json<EventPosition>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    )
    .await?;
    let now = chrono::Utc::now();

    let position = sqlx::query_as::<_, EventPosition>(
        "UPDATE events.event_positions
         SET assigned_slot = $1, status = 'ASSIGNED', user_id = $2, updated_at = $3
         WHERE id = $4 AND event_id = $5
         RETURNING id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at"
    )
    .bind(req.assigned_slot)
    .bind(&req.user_id)
    .bind(&now)
    .bind(&position_id)
    .bind(&event_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    Ok(Json(position))
}

// Delete event position
#[utoipa::path(
    delete,
    path = "/api/v1/events/{event_id}/positions/{position_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID"),
        ("position_id" = String, Path, description = "Position ID")
    ),
    responses(
        (status = 204, description = "Position deleted"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn delete_event_position(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path((event_id, position_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    )
    .await?;

    let result = sqlx::query("DELETE FROM events.event_positions WHERE id = $1 AND event_id = $2")
        .bind(&position_id)
        .bind(&event_id)
        .execute(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest);
    }

    Ok(StatusCode::NO_CONTENT)
}

// Publish positions for event
#[utoipa::path(
    post,
    path = "/api/v1/events/{event_id}/positions/publish",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "Positions published"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn publish_event_positions(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    )
    .await?;

    sqlx::query("UPDATE events.event_positions SET published = true WHERE event_id = $1")
        .bind(&event_id)
        .execute(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::OK)
}
