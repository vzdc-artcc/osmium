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
        permissions::{
            EventsItemsCreate, EventsItemsDelete, EventsItemsUpdate, EventsPositionsAssign,
            EventsPositionsDelete, EventsPositionsPublish, EventsPositionsSelfRequest,
        },
        require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{
        AssignEventPositionRequest, CreateEventPositionRequest, CreateEventRequest, Event,
        EventListResponse, EventPosition, EventPositionListResponse,
        PaginationMeta, PaginationQuery, UpdateEventRequest, UserEventPositionListResponse,
    },
    repos::{audit as audit_repo, events as events_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
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
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EventListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = events_repo::count_events(db).await?;
    let events = events_repo::list_events(db, pagination.page_size, pagination.offset).await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        EventListResponse {
            items: events,
            pagination: meta,
        },
        time,
    ))
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
        (status = 404, description = "Event not found")
    )
)]
pub async fn get_event(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Event>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let event = events_repo::fetch_event(db, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(ApiJson::new(event, time))
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
    _permission: RequirePermission<EventsItemsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(req): Json<CreateEventRequest>,
) -> Result<(StatusCode, ApiJson<Event>), ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    validate_event_window(req.starts_at, req.ends_at)?;

    let event_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let event = events_repo::insert_event(
        db,
        &event_id,
        &req.title,
        req.event_type.as_deref(),
        req.host.as_deref(),
        req.description.as_deref(),
        "SCHEDULED",
        false,
        req.starts_at,
        req.ends_at,
        &user.id,
        now,
    )
    .await?;

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

    Ok((StatusCode::CREATED, ApiJson::new(event, time)))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Event not found")
    )
)]
pub async fn update_event(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<EventsItemsUpdate>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(req): Json<UpdateEventRequest>,
) -> Result<ApiJson<Event>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let now = chrono::Utc::now();
    let before = events_repo::fetch_event(db, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    validate_event_window(
        req.starts_at.unwrap_or(before.starts_at),
        req.ends_at.unwrap_or(before.ends_at),
    )?;

    let event = events_repo::update_event_row(
        db,
        &event_id,
        req.title,
        req.event_type,
        req.host,
        req.description,
        req.status,
        req.published,
        req.starts_at,
        req.ends_at,
        now,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

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

    Ok(ApiJson::new(event, time))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Event not found")
    )
)]
pub async fn delete_event(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<EventsItemsDelete>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let before = events_repo::fetch_event(db, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let rows_affected = events_repo::delete_event_row(db, &event_id).await?;
    if rows_affected == 0 {
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
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EventPositionListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = events_repo::count_event_positions(db, &event_id).await?;
    let positions =
        events_repo::list_event_positions(db, &event_id, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        EventPositionListResponse {
            items: positions,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(get, path = "/api/v1/users/{cid}/event-positions", tag = "events", params(("cid" = i64, Path, description = "User CID")), responses((status = 200, description = "User's published event positions, most recent event first", body = UserEventPositionListResponse), (status = 401, description = "Not authenticated")))]
pub async fn get_user_event_positions(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<UserEventPositionListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    // Same data-dependent authorization as org::get_user_solo_certifications: self-view
    // needs only "auth.profile.read", viewing someone else needs "users.directory.read".
    if user.cid != cid {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
        )
        .await?;
    } else {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
        )
        .await?;
    }
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let items = events_repo::fetch_user_published_event_positions(db, cid).await?;

    Ok(ApiJson::new(UserEventPositionListResponse { items }, time))
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
    _permission: RequirePermission<EventsPositionsSelfRequest>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(req): Json<CreateEventPositionRequest>,
) -> Result<(StatusCode, ApiJson<EventPosition>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let position_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let position = events_repo::insert_event_position(
        db,
        &position_id,
        &event_id,
        &req.callsign,
        &user.id,
        req.requested_slot,
        now,
    )
    .await?;

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

    Ok((StatusCode::CREATED, ApiJson::new(position, time)))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Event or position not found")
    )
)]
pub async fn assign_event_position(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<EventsPositionsAssign>,
    Path((event_id, position_id)): Path<(String, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(req): Json<AssignEventPositionRequest>,
) -> Result<ApiJson<EventPosition>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let now = chrono::Utc::now();
    let before = events_repo::fetch_event_position(db, &position_id, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let position = events_repo::assign_event_position_row(
        db,
        &position_id,
        &event_id,
        req.assigned_slot,
        &req.user_id,
        now,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

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

    Ok(ApiJson::new(position, time))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Event or position not found")
    )
)]
pub async fn delete_event_position(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<EventsPositionsDelete>,
    Path((event_id, position_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let before = events_repo::fetch_event_position(db, &position_id, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let rows_affected = events_repo::delete_event_position_row(db, &position_id, &event_id).await?;
    if rows_affected == 0 {
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
    _permission: RequirePermission<EventsPositionsPublish>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let before = events_repo::list_event_positions_all(db, &event_id).await?;

    events_repo::set_positions_published(db, &event_id).await?;

    let after = events_repo::list_event_positions_all(db, &event_id).await?;

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
