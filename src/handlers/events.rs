use axum::{
    Json,
    extract::Extension,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{
        AssignEventPositionRequest, CreateEventPositionRequest, CreateEventRequest, Event,
        EventListResponse, EventPosition, EventPositionListResponse, ListEventsQuery,
        PaginationMeta, PaginationQuery, UpdateEventRequest,
    },
    repos::audit as audit_repo,
    state::AppState,
};

fn validate_event_window(
    starts_at: chrono::DateTime<chrono::Utc>,
    ends_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), ApiError> {
    if ends_at < starts_at {
        return Err(ApiError::BadRequest);
    }

    Ok(())
}

// List events
#[utoipa::path(
    get,
    path = "/api/v1/events",
    tag = "events",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List events", body = EventListResponse)
    )
)]
pub async fn list_events(
    State(state): State<AppState>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<EventListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = sqlx::query_scalar::<_, i64>("select count(*)::bigint from events.events")
        .fetch_one(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    let events = sqlx::query_as::<_, Event>(
        "SELECT id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at FROM events.events ORDER BY starts_at DESC, id ASC LIMIT $1 OFFSET $2"
    )
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(Json(EventListResponse {
        items: events,
        total: meta.total,
        page: meta.page,
        page_size: meta.page_size,
        total_pages: meta.total_pages,
        has_next: meta.has_next,
        has_prev: meta.has_prev,
    }))
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
    headers: HeaderMap,
    Json(req): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<Event>), ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "items"], PermissionAction::Create),
    )
    .await?;

    validate_event_window(req.starts_at, req.ends_at)?;

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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "CREATE".to_string(),
            resource_type: "EVENT".to_string(),
            resource_id: Some(event.id.clone()),
            scope_type: "event".to_string(),
            scope_key: Some(event.id.clone()),
            before_state: None,
            after_state: Some(audit_repo::sanitized_snapshot(&event)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
    Json(req): Json<UpdateEventRequest>,
) -> Result<Json<Event>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "items"], PermissionAction::Update),
    )
    .await?;
    let now = chrono::Utc::now();
    let before = sqlx::query_as::<_, Event>(
        "SELECT id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at FROM events.events WHERE id = $1"
    )
    .bind(&event_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    validate_event_window(
        req.starts_at.unwrap_or(before.starts_at),
        req.ends_at.unwrap_or(before.ends_at),
    )?;

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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "EVENT".to_string(),
            resource_id: Some(event.id.clone()),
            scope_type: "event".to_string(),
            scope_key: Some(event.id.clone()),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&event)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "items"], PermissionAction::Delete),
    )
    .await?;

    let before = sqlx::query_as::<_, Event>(
        "SELECT id, title, type AS event_type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at FROM events.events WHERE id = $1"
    )
    .bind(&event_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let result = sqlx::query("DELETE FROM events.events WHERE id = $1")
        .bind(&event_id)
        .execute(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest);
    }

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "DELETE".to_string(),
            resource_type: "EVENT".to_string(),
            resource_id: Some(before.id.clone()),
            scope_type: "event".to_string(),
            scope_key: Some(before.id.clone()),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: None,
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::validate_event_window;
    use crate::errors::ApiError;

    #[test]
    fn validate_event_window_accepts_equal_or_forward_ranges() {
        let starts_at = Utc.with_ymd_and_hms(2026, 5, 7, 3, 0, 0).unwrap();
        let ends_at = Utc.with_ymd_and_hms(2026, 5, 7, 5, 0, 0).unwrap();

        assert!(validate_event_window(starts_at, starts_at).is_ok());
        assert!(validate_event_window(starts_at, ends_at).is_ok());
    }

    #[test]
    fn validate_event_window_rejects_reversed_ranges() {
        let starts_at = Utc.with_ymd_and_hms(2026, 5, 7, 3, 0, 0).unwrap();
        let ends_at = Utc.with_ymd_and_hms(2026, 5, 6, 23, 0, 0).unwrap();

        assert!(matches!(
            validate_event_window(starts_at, ends_at),
            Err(ApiError::BadRequest)
        ));
    }
}

// List event positions
#[utoipa::path(
    get,
    path = "/api/v1/events/{event_id}/positions",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID"),
        PaginationQuery
    ),
    responses(
        (status = 200, description = "List event positions", body = EventPositionListResponse),
        (status = 400, description = "Invalid event ID")
    )
)]
pub async fn list_event_positions(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<EventPositionListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM events.event_positions WHERE event_id = $1",
    )
    .bind(&event_id)
    .fetch_one(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let positions = sqlx::query_as::<_, EventPosition>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE event_id = $1 ORDER BY assigned_slot ASC NULLS LAST, id ASC LIMIT $2 OFFSET $3"
    )
    .bind(&event_id)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(Json(EventPositionListResponse {
        items: positions,
        total: meta.total,
        page: meta.page,
        page_size: meta.page_size,
        total_pages: meta.total_pages,
        has_next: meta.has_next,
        has_prev: meta.has_prev,
    }))
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
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_event_position(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<CreateEventPositionRequest>,
) -> Result<(StatusCode, Json<EventPosition>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "positions", "self"], PermissionAction::Request),
    )
    .await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let position_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let position = sqlx::query_as::<_, EventPosition>(
        "INSERT INTO events.event_positions (id, event_id, callsign, user_id, requested_slot, status, published, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at"
    )
    .bind(&position_id)
    .bind(&event_id)
    .bind(&req.callsign)
    .bind(&user.id)
    .bind(&req.requested_slot)
    .bind("REQUESTED")
    .bind(false)
    .bind(&now)
    .bind(&now)
    .fetch_one(db)
    .await
    .map_err(|error| match error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => ApiError::BadRequest,
        _ => ApiError::Internal,
    })?;

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "CREATE".to_string(),
            resource_type: "EVENT_POSITION".to_string(),
            resource_id: Some(position.id.clone()),
            scope_type: "event".to_string(),
            scope_key: Some(event_id),
            before_state: None,
            after_state: Some(audit_repo::sanitized_snapshot(&position)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
    Json(req): Json<AssignEventPositionRequest>,
) -> Result<Json<EventPosition>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "positions"], PermissionAction::Assign),
    )
    .await?;
    let now = chrono::Utc::now();
    let before = sqlx::query_as::<_, EventPosition>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE id = $1 AND event_id = $2"
    )
    .bind(&position_id)
    .bind(&event_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "ASSIGN".to_string(),
            resource_type: "EVENT_POSITION".to_string(),
            resource_id: Some(position.id.clone()),
            scope_type: "event".to_string(),
            scope_key: Some(event_id),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&position)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "positions"], PermissionAction::Delete),
    )
    .await?;

    let before = sqlx::query_as::<_, EventPosition>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE id = $1 AND event_id = $2"
    )
    .bind(&position_id)
    .bind(&event_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let result = sqlx::query("DELETE FROM events.event_positions WHERE id = $1 AND event_id = $2")
        .bind(&position_id)
        .bind(&event_id)
        .execute(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest);
    }

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "DELETE".to_string(),
            resource_type: "EVENT_POSITION".to_string(),
            resource_id: Some(before.id.clone()),
            scope_type: "event".to_string(),
            scope_key: Some(event_id),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: None,
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["events", "positions"], PermissionAction::Publish),
    )
    .await?;

    let before = sqlx::query_as::<_, EventPosition>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE event_id = $1 ORDER BY assigned_slot ASC NULLS LAST"
    )
    .bind(&event_id)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query("UPDATE events.event_positions SET published = true WHERE event_id = $1")
        .bind(&event_id)
        .execute(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    let after = sqlx::query_as::<_, EventPosition>(
        "SELECT id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at FROM events.event_positions WHERE event_id = $1 ORDER BY assigned_slot ASC NULLS LAST"
    )
    .bind(&event_id)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "PUBLISH".to_string(),
            resource_type: "EVENT_POSITION_BATCH".to_string(),
            resource_id: None,
            scope_type: "event".to_string(),
            scope_key: Some(event_id),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(StatusCode::OK)
}
