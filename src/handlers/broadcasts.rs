use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    auth::{
        context::{CurrentServiceAccount, CurrentUser},
        permissions::{
            AuthProfileRead, AuthProfileUpdate, WebBroadcastsCreate, WebBroadcastsDelete,
            WebBroadcastsRead, WebBroadcastsUpdate,
        },
        require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{
        ChangeBroadcastListItem, ChangeBroadcastListResponse, CreateChangeBroadcastRequest,
        ListChangeBroadcastsQuery, MyChangeBroadcastListResponse, PaginationMeta, PaginationQuery,
        UpdateChangeBroadcastRequest,
    },
    repos::{audit as audit_repo, broadcasts as broadcasts_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[utoipa::path(
    get,
    path = "/api/v1/admin/broadcasts",
    tag = "broadcasts",
    params(
        PaginationQuery,
        ("title" = Option<String>, Query, description = "Filter by title substring"),
        ("exempt_staff" = Option<bool>, Query, description = "Filter by exempt-staff flag")
    ),
    responses(
        (status = 200, description = "List change broadcasts", body = ChangeBroadcastListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_broadcasts(
    State(state): State<AppState>,
    _permission: RequirePermission<WebBroadcastsRead>,
    Query(query): Query<ListChangeBroadcastsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ChangeBroadcastListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let title_pattern = query
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"));

    let total =
        broadcasts_repo::count_broadcasts(pool, title_pattern.as_deref(), query.exempt_staff)
            .await?;
    let items: Vec<ChangeBroadcastListItem> = broadcasts_repo::list_broadcasts(
        pool,
        title_pattern.as_deref(),
        query.exempt_staff,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        ChangeBroadcastListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/broadcasts",
    tag = "broadcasts",
    request_body = CreateChangeBroadcastRequest,
    responses(
        (status = 201, description = "Change broadcast created", body = ChangeBroadcastListItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_broadcast(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<WebBroadcastsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateChangeBroadcastRequest>,
) -> Result<(StatusCode, ApiJson<ChangeBroadcastListItem>), ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let title = normalize_required(&payload.title)?;
    let description = normalize_required(&payload.description)?;
    let file_id = normalize_optional(payload.file_id.as_deref());

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();

    broadcasts_repo::insert_broadcast(
        &mut *tx,
        &id,
        &title,
        &description,
        file_id.as_deref(),
        payload.exempt_staff,
        now,
    )
    .await?;

    if payload.exempt_staff {
        broadcasts_repo::insert_staff_agreed_state(&mut *tx, &id, now).await?;
    }

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        "CREATE",
        &id,
        None::<&broadcasts_repo::BroadcastRow>,
        Some(&payload),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let item = fetch_list_item(pool, &id).await?;
    Ok((StatusCode::CREATED, ApiJson::new(item, time)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/broadcasts/{broadcast_id}",
    tag = "broadcasts",
    params(
        ("broadcast_id" = String, Path, description = "Broadcast ID")
    ),
    request_body = UpdateChangeBroadcastRequest,
    responses(
        (status = 200, description = "Change broadcast updated", body = ChangeBroadcastListItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Broadcast not found")
    )
)]
pub async fn update_broadcast(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<WebBroadcastsUpdate>,
    Path(broadcast_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateChangeBroadcastRequest>,
) -> Result<ApiJson<ChangeBroadcastListItem>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let title = normalize_required(&payload.title)?;
    let description = normalize_required(&payload.description)?;
    let file_id = normalize_optional(payload.file_id.as_deref());

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let before = broadcasts_repo::fetch_broadcast_row(&mut *tx, &broadcast_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    broadcasts_repo::update_broadcast_row(
        &mut *tx,
        &broadcast_id,
        &title,
        &description,
        file_id.as_deref(),
        payload.exempt_staff,
        Utc::now(),
    )
    .await?;

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        "UPDATE",
        &broadcast_id,
        Some(&before),
        Some(&payload),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let item = fetch_list_item(pool, &broadcast_id).await?;
    Ok(ApiJson::new(item, time))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/broadcasts/{broadcast_id}",
    tag = "broadcasts",
    params(
        ("broadcast_id" = String, Path, description = "Broadcast ID")
    ),
    responses(
        (status = 204, description = "Change broadcast deleted"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Broadcast not found")
    )
)]
pub async fn delete_broadcast(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<WebBroadcastsDelete>,
    Path(broadcast_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let before = broadcasts_repo::fetch_broadcast_row(&mut *tx, &broadcast_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    broadcasts_repo::delete_broadcast_row(&mut *tx, &broadcast_id).await?;

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        "DELETE",
        &broadcast_id,
        Some(&before),
        None::<&UpdateChangeBroadcastRequest>,
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/broadcasts/me",
    tag = "broadcasts",
    responses(
        (status = 200, description = "Change broadcasts with the current user's status", body = MyChangeBroadcastListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_my_broadcasts(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    time: ResponseTimeContext,
) -> Result<ApiJson<MyChangeBroadcastListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let items = broadcasts_repo::fetch_my_broadcasts(pool, &user.id).await?;

    Ok(ApiJson::new(MyChangeBroadcastListResponse { items }, time))
}

#[utoipa::path(
    post,
    path = "/api/v1/broadcasts/{broadcast_id}/seen",
    tag = "broadcasts",
    params(
        ("broadcast_id" = String, Path, description = "Broadcast ID")
    ),
    responses(
        (status = 204, description = "Broadcast marked as seen"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn mark_broadcast_seen(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileUpdate>,
    Path(broadcast_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    broadcasts_repo::upsert_seen_state(pool, &broadcast_id, &user.id, Utc::now()).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/broadcasts/{broadcast_id}/agree",
    tag = "broadcasts",
    params(
        ("broadcast_id" = String, Path, description = "Broadcast ID")
    ),
    responses(
        (status = 204, description = "Broadcast marked as agreed"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn mark_broadcast_agreed(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileUpdate>,
    Path(broadcast_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    broadcasts_repo::upsert_agreed_state(pool, &broadcast_id, &user.id, Utc::now()).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn fetch_list_item(
    pool: &sqlx::PgPool,
    broadcast_id: &str,
) -> Result<ChangeBroadcastListItem, ApiError> {
    broadcasts_repo::fetch_broadcast_list_item(pool, broadcast_id)
        .await?
        .ok_or(ApiError::Internal)
}

fn normalize_required(value: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest);
    }
    Ok(trimmed.to_string())
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[allow(clippy::too_many_arguments)]
async fn record_audit_entry<TBefore, TAfter>(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
    action: &str,
    broadcast_id: &str,
    before_state: Option<&TBefore>,
    after_state: Option<&TAfter>,
    headers: &HeaderMap,
) -> Result<(), ApiError>
where
    TBefore: serde::Serialize,
    TAfter: serde::Serialize,
{
    let actor =
        audit_repo::resolve_audit_actor(&mut **tx, current_user, current_service_account).await?;
    audit_repo::record_audit(
        &mut **tx,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.to_string(),
            resource_type: "CHANGE_BROADCAST".to_string(),
            resource_id: Some(broadcast_id.to_string()),
            scope_type: "web".to_string(),
            scope_key: Some(broadcast_id.to_string()),
            before_state: before_state
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: after_state
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}
