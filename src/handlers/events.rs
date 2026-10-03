use axum::{
    Json,
    extract::Extension,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use std::collections::HashSet;

use serde_json::json;
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
    email::service::EmailActor,
    errors::ApiError,
    models::{
        CreateEventPositionRequest, CreateEventRequest, Event, EventListResponse, EventPosition,
        EventPositionListResponse, PaginationMeta, PaginationQuery, UpdateEventPositionRequest,
        UpdateEventRequest, UserEventPositionListResponse,
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
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks events.items.create")
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
        req.banner_asset_id.as_deref(),
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
            message: None,
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
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks events.items.update"),
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
        req.banner_asset_id.is_some(),
        req.banner_asset_id.flatten(),
        req.hidden,
        req.manual_positions_open,
        req.archived,
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
            message: None,
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
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks events.items.delete"),
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
            message: None,
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

#[utoipa::path(get, path = "/api/v1/users/{cid}/event-positions", tag = "events", params(("cid" = i64, Path, description = "User CID")), responses((status = 200, description = "User's published event positions, most recent event first", body = UserEventPositionListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Own positions need auth.profile.read; someone else's need users.directory.read")))]
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

// Create event position (self-service signup, or admin manual-add on behalf of another user)
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
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Lacks events.positions.self.request, or events.positions.assign when creating on behalf of another user")
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

    // Admin manual-add: creating on behalf of a different user requires the
    // staff assign permission on top of the self-request permission every
    // caller already has.
    let target_user_id = match req.user_id.as_deref() {
        Some(target) if target != user.id => {
            ensure_permission(
                &state,
                Some(user),
                None,
                PermissionPath::from_segments(["events", "positions"], PermissionAction::Assign),
            )
            .await?;
            target.to_string()
        }
        Some(_) | None => user.id.clone(),
    };
    let is_manual_add = target_user_id != user.id;

    let position_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let position = events_repo::insert_event_position(
        db,
        &position_id,
        &event_id,
        &target_user_id,
        &req.requested_position,
        req.requested_secondary_position
            .as_deref()
            .unwrap_or("UNKNOWN"),
        req.notes.as_deref(),
        req.requested_start_time,
        req.requested_end_time,
        req.final_position.as_deref(),
        req.final_start_time,
        req.final_end_time,
        req.final_notes.as_deref(),
        req.controlling_category.as_deref(),
        req.is_instructor.unwrap_or(false),
        req.is_solo.unwrap_or(false),
        req.is_ots.unwrap_or(false),
        req.is_tmu.unwrap_or(false),
        req.is_cic.unwrap_or(false),
        if is_manual_add {
            "ASSIGNED"
        } else {
            "REQUESTED"
        },
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
            message: None,
            before_state: None,
            after_state: Some(audit_repo::sanitized_snapshot(&position)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(position, time)))
}

// Update event position (staff only) — reassign, finalize, publish/unpublish, or
// change status of an existing signup.
#[utoipa::path(
    patch,
    path = "/api/v1/events/{event_id}/positions/{position_id}",
    tag = "events",
    params(
        ("event_id" = String, Path, description = "Event ID"),
        ("position_id" = String, Path, description = "Position ID")
    ),
    request_body = UpdateEventPositionRequest,
    responses(
        (status = 200, description = "Position updated", body = EventPosition),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks events.positions.assign"),
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
    Json(req): Json<UpdateEventPositionRequest>,
) -> Result<ApiJson<EventPosition>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let now = chrono::Utc::now();
    let before = events_repo::fetch_event_position(db, &position_id, &event_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Assigning a real user (not clearing the assignment) defaults status to
    // ASSIGNED unless the caller explicitly overrides it — matches the
    // historical "assign a slot" behavior this endpoint started as.
    let status = req
        .status
        .clone()
        .or_else(|| matches!(req.user_id, Some(Some(_))).then(|| "ASSIGNED".to_string()));

    let position = events_repo::update_event_position_row(
        db,
        &position_id,
        &event_id,
        req.user_id.is_some(),
        req.user_id.flatten(),
        req.assigned_slot,
        req.final_position.is_some(),
        req.final_position.flatten(),
        req.final_start_time.is_some(),
        req.final_start_time.flatten(),
        req.final_end_time.is_some(),
        req.final_end_time.flatten(),
        req.final_notes.is_some(),
        req.final_notes.flatten(),
        req.controlling_category.is_some(),
        req.controlling_category.flatten(),
        req.is_instructor,
        req.is_solo,
        req.is_ots,
        req.is_tmu,
        req.is_cic,
        req.published,
        status,
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
            resource_type: "EVENT_POSITION".to_string(),
            resource_id: Some(position.id.clone()),
            scope_type: "event".to_string(),
            scope_key: Some(event_id),
            message: None,
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
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks events.positions.delete"),
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
            message: None,
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
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks events.positions.publish")
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
    let actor_id_for_email = actor.actor_id.clone();
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "PUBLISH".to_string(),
            resource_type: "EVENT_POSITION_BATCH".to_string(),
            resource_id: None,
            scope_type: "event".to_string(),
            scope_key: Some(event_id.clone()),
            message: None,
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    // Notify each controller whose assigned position was *newly* published (the
    // batch-publish flips the whole event's positions, so diff against `before` to
    // avoid re-notifying already-published assignments). Best-effort — a mail
    // failure must not fail the publish. Respects each user's event-notification
    // preference (the template's respect_user_event_pref).
    notify_newly_published_positions(&state, db, &event_id, &before, &after, actor_id_for_email)
        .await;

    Ok(StatusCode::OK)
}

/// Enqueue `events.position_published` to controllers whose assigned position
/// transitioned to published in this batch.
async fn notify_newly_published_positions(
    state: &AppState,
    db: &sqlx::PgPool,
    event_id: &str,
    before: &[EventPosition],
    after: &[EventPosition],
    actor_id: Option<String>,
) {
    let previously_published: HashSet<&str> = before
        .iter()
        .filter(|position| position.published)
        .map(|position| position.id.as_str())
        .collect();

    let mut recipients: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for position in after {
        if !position.published || previously_published.contains(position.id.as_str()) {
            continue;
        }
        if let Some(user_id) = &position.user_id {
            if seen.insert(user_id.clone()) {
                recipients.push(user_id.clone());
            }
        }
    }

    if recipients.is_empty() {
        return;
    }

    let Ok(Some(event)) = events_repo::fetch_event(db, event_id).await else {
        return;
    };

    // `unsubscribe_base_url` is this deployment's public base URL (see
    // email/render.rs); fall back to a relative path if it isn't configured.
    let details_url = match state.email.config.unsubscribe_base_url.as_deref() {
        Some(base) => format!("{}/events/{}", base.trim_end_matches('/'), event_id),
        None => format!("/events/{event_id}"),
    };

    let payload = json!({
        "event_title": event.title,
        "starts_at": event.starts_at.to_rfc3339(),
        "details_url": details_url,
        "preheader": format!("Your position for {} has been published", event.title),
    });

    let email_actor = EmailActor {
        actor_id,
        user_id: None,
        service_account_id: None,
        request_source: "system".to_string(),
    };

    if let Err(error) = state
        .email
        .enqueue_to_users(
            db,
            email_actor,
            "events.position_published".to_string(),
            payload,
            recipients,
        )
        .await
    {
        tracing::warn!(
            ?error,
            event_id,
            "failed to enqueue event position-published emails"
        );
    }
}
