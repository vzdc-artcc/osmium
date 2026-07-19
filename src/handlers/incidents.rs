use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        context::CurrentUser,
        permissions::{FeedbackItemsCreate, FeedbackItemsDecide, FeedbackItemsSelfRead},
        require_permission::RequirePermission,
    },
    email::service::actor_from_context,
    errors::ApiError,
    models::{
        CreateIncidentRequest, IncidentItem, IncidentListResponse, ListIncidentsQuery,
        PaginationMeta, PaginationQuery, UpdateIncidentRequest,
    },
    repos::{audit as audit_repo, incidents as incidents_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/incidents",
    tag = "incidents",
    request_body = CreateIncidentRequest,
    responses(
        (status = 201, description = "Incident created", body = IncidentItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_incident(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FeedbackItemsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateIncidentRequest>,
) -> Result<(StatusCode, ApiJson<IncidentItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.reason.trim().is_empty()
        || payload.reportee_callsign.trim().is_empty()
        || payload.timestamp > Utc::now()
    {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let item = incidents_repo::insert_incident(
        pool,
        &Uuid::new_v4().to_string(),
        &user.id,
        &payload.reportee_id,
        payload.timestamp,
        payload.reason.trim(),
        payload
            .reporter_callsign
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        payload.reportee_callsign.trim(),
    )
    .await?;

    let full = incidents_repo::fetch_incident(pool, &item.id).await?;
    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "INCIDENT",
        Some(full.id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&full)?),
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(full, time)))
}

#[utoipa::path(
    get,
    path = "/api/v1/incidents",
    tag = "incidents",
    params(PaginationQuery, ("closed" = Option<bool>, Query, description = "Optional closed-state filter")),
    responses(
        (status = 200, description = "Incidents involving the current user", body = IncidentListResponse),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_my_incidents(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FeedbackItemsSelfRead>,
    Query(query): Query<ListIncidentsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<IncidentListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);

    let total = incidents_repo::count_my_incidents(pool, &user.id, query.closed).await?;
    let items = incidents_repo::list_my_incidents(
        pool,
        &user.id,
        query.closed,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        IncidentListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/incidents",
    tag = "incidents",
    params(PaginationQuery, ("closed" = Option<bool>, Query, description = "Optional closed-state filter")),
    responses(
        (status = 200, description = "Incident list for staff review", body = IncidentListResponse),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn admin_list_incidents(
    State(state): State<AppState>,
    _permission: RequirePermission<FeedbackItemsDecide>,
    Query(query): Query<ListIncidentsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<IncidentListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);

    let total = incidents_repo::count_all_incidents(pool, query.closed).await?;
    let items = incidents_repo::list_all_incidents(
        pool,
        query.closed,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        IncidentListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/incidents/{incident_id}",
    tag = "incidents",
    params(
        ("incident_id" = String, Path, description = "Incident ID")
    ),
    responses(
        (status = 200, description = "Incident detail", body = IncidentItem),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Incident not found")
    )
)]
pub async fn admin_get_incident(
    State(state): State<AppState>,
    _permission: RequirePermission<FeedbackItemsDecide>,
    Path(incident_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<IncidentItem>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(ApiJson::new(
        incidents_repo::fetch_incident(pool, &incident_id).await?,
        time,
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/incidents/{incident_id}",
    tag = "incidents",
    params(
        ("incident_id" = String, Path, description = "Incident ID")
    ),
    request_body = UpdateIncidentRequest,
    responses(
        (status = 200, description = "Updated incident", body = IncidentItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Incident not found")
    )
)]
pub async fn admin_update_incident(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FeedbackItemsDecide>,
    Path(incident_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateIncidentRequest>,
) -> Result<ApiJson<IncidentItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = incidents_repo::fetch_incident(pool, &incident_id).await?;
    if before.closed && payload.closed {
        return Err(ApiError::BadRequest);
    }

    let item = incidents_repo::update_incident_closed(pool, &incident_id, payload.closed)
        .await?
        .ok_or(ApiError::NotFound)?;

    if payload.closed {
        let resolved_actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
        let email_actor = actor_from_context(Some(user), None, resolved_actor.actor_id, "api");
        let _ = state
            .email
            .enqueue_to_users(
                pool,
                email_actor,
                "incident.closed".to_string(),
                json!({
                    "controller_name": item.reportee_name.clone().unwrap_or_else(|| "Controller".to_string()),
                    "incident_date": item.timestamp.format("%Y-%m-%d").to_string(),
                    "resolution": payload.resolution
                }),
                vec![item.reportee_id.clone()],
            )
            .await;
    }

    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "INCIDENT",
        Some(incident_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&item)?),
    )
    .await?;

    Ok(ApiJson::new(item, time))
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
            scope_type: "global".to_string(),
            scope_key: Some(user.cid.to_string()),
            before_state,
            after_state,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}
