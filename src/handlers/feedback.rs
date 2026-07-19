use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        permissions::{FeedbackItemsCreate, FeedbackItemsDecide},
        require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{
        CreateFeedbackRequest, DecideFeedbackRequest, FeedbackItem, FeedbackListQuery,
        FeedbackListResponse, PaginationMeta, PaginationQuery,
    },
    repos::{audit as audit_repo, feedback as feedback_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[utoipa::path(
    post,
    path = "/api/v1/feedback",
    tag = "feedback",
    request_body = CreateFeedbackRequest,
    responses(
        (status = 201, description = "Feedback created", body = FeedbackItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Target user not found")
    )
)]
pub async fn create_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FeedbackItemsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateFeedbackRequest>,
) -> Result<(StatusCode, ApiJson<FeedbackItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    if payload.rating < 1 || payload.rating > 5 {
        return Err(ApiError::BadRequest);
    }

    let pilot_callsign = payload.pilot_callsign.trim();
    let controller_position = payload.controller_position.trim();
    if pilot_callsign.is_empty() || controller_position.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let target_user_id = feedback_repo::find_user_id_by_cid(pool, payload.target_cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let feedback_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let item = feedback_repo::insert_feedback_item(
        pool,
        &feedback_id,
        &user.id,
        &target_user_id,
        pilot_callsign,
        controller_position,
        payload.rating,
        payload.comments.as_deref(),
        now,
    )
    .await?;

    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "CREATE".to_string(),
            resource_type: "FEEDBACK".to_string(),
            resource_id: Some(item.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(target_user_id),
            before_state: None,
            after_state: Some(audit_repo::sanitized_snapshot(&item)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(item, time)))
}

#[utoipa::path(
    get,
    path = "/api/v1/feedback",
    tag = "feedback",
    params(
        PaginationQuery,
        ("status" = Option<String>, Query, description = "Optional feedback status")
    ),
    responses(
        (status = 200, description = "Feedback list", body = FeedbackListResponse),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<FeedbackListQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<FeedbackListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    // Not a RequirePermission<P> case: which of the two permissions the caller holds
    // changes the query scope below (all items vs. only their own), not just whether
    // the request is allowed at all, so this stays a manual, data-dependent check.
    let (_, permissions) = crate::auth::acl::fetch_user_access(state.db.as_ref(), &user.id).await?;
    let can_read_all = permissions.contains(&PermissionPath::from_segments(
        ["feedback", "items"],
        PermissionAction::Read,
    ));
    let can_read_self = permissions.contains(&PermissionPath::from_segments(
        ["feedback", "items", "self"],
        PermissionAction::Read,
    ));

    if !can_read_all && !can_read_self {
        return Err(ApiError::Unauthorized);
    }

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 500);
    let normalized_status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .map_or(Ok(None), |normalized| {
            if normalized != "PENDING" && normalized != "RELEASED" && normalized != "STASHED" {
                Err(ApiError::BadRequest)
            } else {
                Ok(Some(normalized))
            }
        })?;

    let total = if can_read_all {
        feedback_repo::count_all(pool, normalized_status.as_deref()).await?
    } else {
        feedback_repo::count_by_submitter(pool, &user.id, normalized_status.as_deref()).await?
    };

    let items = if can_read_all {
        feedback_repo::list_all(
            pool,
            normalized_status.as_deref(),
            pagination.page_size,
            pagination.offset,
        )
        .await?
    } else {
        feedback_repo::list_by_submitter(
            pool,
            &user.id,
            normalized_status.as_deref(),
            pagination.page_size,
            pagination.offset,
        )
        .await?
    };

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        FeedbackListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/feedback/{feedback_id}",
    tag = "feedback",
    params(
        ("feedback_id" = String, Path, description = "Feedback record ID")
    ),
    request_body = DecideFeedbackRequest,
    responses(
        (status = 200, description = "Feedback decision applied", body = FeedbackItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Feedback record not found")
    )
)]
pub async fn decide_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FeedbackItemsDecide>,
    Path(feedback_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<DecideFeedbackRequest>,
) -> Result<ApiJson<FeedbackItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "PENDING"
        && normalized_status != "RELEASED"
        && normalized_status != "STASHED"
    {
        return Err(ApiError::BadRequest);
    }

    let now = chrono::Utc::now();
    let before = feedback_repo::find_by_id(pool, &feedback_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let item = feedback_repo::update_decision(
        pool,
        &feedback_id,
        &normalized_status,
        payload.staff_comments.as_deref(),
        now,
        &user.id,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "DECIDE".to_string(),
            resource_type: "FEEDBACK".to_string(),
            resource_id: Some(item.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(item.target_user_id.clone()),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&item)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(ApiJson::new(item, time))
}
