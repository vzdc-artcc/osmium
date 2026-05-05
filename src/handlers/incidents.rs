use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    email::service::actor_from_context,
    errors::ApiError,
    repos::audit as audit_repo,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct IncidentItem {
    pub id: String,
    pub reporter_id: String,
    pub reportee_id: String,
    pub timestamp: DateTime<Utc>,
    pub reason: String,
    pub closed: bool,
    pub reporter_callsign: Option<String>,
    pub reportee_callsign: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub reporter_cid: Option<i64>,
    pub reporter_name: Option<String>,
    pub reportee_cid: Option<i64>,
    pub reportee_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateIncidentRequest {
    pub reportee_id: String,
    pub timestamp: DateTime<Utc>,
    pub reason: String,
    pub reporter_callsign: Option<String>,
    pub reportee_callsign: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateIncidentRequest {
    pub closed: bool,
    pub resolution: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListIncidentsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub closed: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IncidentListResponse {
    pub items: Vec<IncidentItem>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

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
    headers: HeaderMap,
    Json(payload): Json<CreateIncidentRequest>,
) -> Result<(StatusCode, Json<IncidentItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["feedback", "items"], PermissionAction::Create),
    )
    .await?;
    if payload.reason.trim().is_empty()
        || payload.reportee_callsign.trim().is_empty()
        || payload.timestamp > Utc::now()
    {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let item = sqlx::query_as::<_, IncidentItem>(
        r#"
        insert into feedback.incident_reports (
            id,
            reporter_id,
            reportee_id,
            timestamp,
            reason,
            closed,
            reporter_callsign,
            reportee_callsign,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, false, $6, $7, now(), now())
        returning
            id,
            reporter_id,
            reportee_id,
            timestamp,
            reason,
            closed,
            reporter_callsign,
            reportee_callsign,
            created_at,
            updated_at,
            null::bigint as reporter_cid,
            null::text as reporter_name,
            null::bigint as reportee_cid,
            null::text as reportee_name
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&user.id)
    .bind(&payload.reportee_id)
    .bind(payload.timestamp)
    .bind(payload.reason.trim())
    .bind(
        payload
            .reporter_callsign
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )
    .bind(payload.reportee_callsign.trim())
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    let full = fetch_incident(pool, &item.id).await?;
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

    Ok((StatusCode::CREATED, Json(full)))
}

#[utoipa::path(
    get,
    path = "/api/v1/incidents",
    tag = "incidents",
    params(ListIncidentsQuery),
    responses(
        (status = 200, description = "Incidents involving the current user", body = IncidentListResponse),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_my_incidents(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListIncidentsQuery>,
) -> Result<Json<IncidentListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["feedback", "items", "self"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from feedback.incident_reports
        where (reporter_id = $1 or reportee_id = $1)
          and ($2::bool is null or closed = $2)
        "#,
    )
    .bind(&user.id)
    .bind(query.closed)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let items = sqlx::query_as::<_, IncidentItem>(
        r#"
        select
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where (i.reporter_id = $1 or i.reportee_id = $1)
          and ($2::bool is null or i.closed = $2)
        order by i.timestamp desc, i.created_at desc
        limit $3 offset $4
        "#,
    )
    .bind(&user.id)
    .bind(query.closed)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(IncidentListResponse {
        items,
        total,
        limit,
        offset,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/incidents",
    tag = "incidents",
    params(ListIncidentsQuery),
    responses(
        (status = 200, description = "Incident list for staff review", body = IncidentListResponse),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn admin_list_incidents(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListIncidentsQuery>,
) -> Result<Json<IncidentListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["feedback", "items"], PermissionAction::Decide),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let total = sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from feedback.incident_reports where ($1::bool is null or closed = $1)",
    )
    .bind(query.closed)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let items = sqlx::query_as::<_, IncidentItem>(
        r#"
        select
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where ($1::bool is null or i.closed = $1)
        order by i.timestamp desc, i.created_at desc
        limit $2 offset $3
        "#,
    )
    .bind(query.closed)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(IncidentListResponse {
        items,
        total,
        limit,
        offset,
    }))
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
        (status = 400, description = "Invalid incident ID"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn admin_get_incident(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(incident_id): Path<String>,
) -> Result<Json<IncidentItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["feedback", "items"], PermissionAction::Decide),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(Json(fetch_incident(pool, &incident_id).await?))
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
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn admin_update_incident(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(incident_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateIncidentRequest>,
) -> Result<Json<IncidentItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["feedback", "items"], PermissionAction::Decide),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_incident(pool, &incident_id).await?;
    if before.closed && payload.closed {
        return Err(ApiError::BadRequest);
    }

    let item = sqlx::query_as::<_, IncidentItem>(
        r#"
        update feedback.incident_reports i
        set closed = $2,
            updated_at = now()
        from identity.users ru, identity.users tu
        where i.id = $1
          and ru.id = i.reporter_id
          and tu.id = i.reportee_id
        returning
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        "#,
    )
    .bind(&incident_id)
    .bind(payload.closed)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

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

    Ok(Json(item))
}

async fn fetch_incident(pool: &sqlx::PgPool, incident_id: &str) -> Result<IncidentItem, ApiError> {
    sqlx::query_as::<_, IncidentItem>(
        r#"
        select
            i.id,
            i.reporter_id,
            i.reportee_id,
            i.timestamp,
            i.reason,
            i.closed,
            i.reporter_callsign,
            i.reportee_callsign,
            i.created_at,
            i.updated_at,
            ru.cid as reporter_cid,
            ru.display_name as reporter_name,
            tu.cid as reportee_cid,
            tu.display_name as reportee_name
        from feedback.incident_reports i
        join identity.users ru on ru.id = i.reporter_id
        join identity.users tu on tu.id = i.reportee_id
        where i.id = $1
        "#,
    )
    .bind(incident_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)
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
