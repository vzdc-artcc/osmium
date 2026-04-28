use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionKey, PermissionResource},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{CreateFeedbackRequest, DecideFeedbackRequest, FeedbackItem},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub struct FeedbackListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/feedback",
    tag = "feedback",
    request_body = CreateFeedbackRequest,
    responses(
        (status = 201, description = "Feedback created", body = FeedbackItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateFeedbackRequest>,
) -> Result<(StatusCode, Json<FeedbackItem>), ApiError> {
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

    let target_user_id =
        sqlx::query_scalar::<_, String>("select id from identity.users where cid = $1")
            .bind(payload.target_cid)
            .fetch_optional(pool)
            .await
            .map_err(|_| ApiError::Internal)?
            .ok_or(ApiError::BadRequest)?;

    let feedback_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let item = sqlx::query_as::<_, FeedbackItem>(
        r#"
        insert into feedback.feedback_items (
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            status,
            submitted_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, 'PENDING', $8)
        returning
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        "#,
    )
    .bind(&feedback_id)
    .bind(&user.id)
    .bind(&target_user_id)
    .bind(pilot_callsign)
    .bind(controller_position)
    .bind(payload.rating)
    .bind(payload.comments.as_deref())
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(item)))
}

#[utoipa::path(
    get,
    path = "/api/v1/feedback",
    tag = "feedback",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum rows"),
        ("offset" = Option<i64>, Query, description = "Pagination offset"),
        ("status" = Option<String>, Query, description = "Optional feedback status")
    ),
    responses(
        (status = 200, description = "Feedback list", body = [FeedbackItem]),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<FeedbackListQuery>,
) -> Result<Json<Vec<FeedbackItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let (_, permissions) = crate::auth::acl::fetch_user_access(state.db.as_ref(), &user.id).await?;
    let can_manage = permissions.contains(&PermissionKey::new(
        PermissionResource::Feedback,
        PermissionAction::Update,
    ));

    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
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

    let items = if can_manage {
        sqlx::query_as::<_, FeedbackItem>(
            r#"
            select
                id,
                submitter_user_id,
                target_user_id,
                pilot_callsign,
                controller_position,
                rating,
                comments,
                staff_comments,
                status,
                submitted_at,
                decided_at,
                decided_by
            from feedback.feedback_items
            where ($1::text is null or status = $1)
            order by submitted_at desc
            limit $2 offset $3
            "#,
        )
        .bind(normalized_status.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?
    } else {
        sqlx::query_as::<_, FeedbackItem>(
            r#"
            select
                id,
                submitter_user_id,
                target_user_id,
                pilot_callsign,
                controller_position,
                rating,
                comments,
                staff_comments,
                status,
                submitted_at,
                decided_at,
                decided_by
            from feedback.feedback_items
            where submitter_user_id = $1
              and ($2::text is null or status = $2)
            order by submitted_at desc
            limit $3 offset $4
            "#,
        )
        .bind(&user.id)
        .bind(normalized_status.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?
    };

    Ok(Json(items))
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
        (status = 401, description = "Not authorized")
    )
)]
pub async fn decide_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(feedback_id): Path<String>,
    Json(payload): Json<DecideFeedbackRequest>,
) -> Result<Json<FeedbackItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Feedback, PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "PENDING"
        && normalized_status != "RELEASED"
        && normalized_status != "STASHED"
    {
        return Err(ApiError::BadRequest);
    }

    let now = chrono::Utc::now();

    let item = sqlx::query_as::<_, FeedbackItem>(
        r#"
        update feedback.feedback_items
        set status = $1,
            staff_comments = $2,
            decided_at = $3,
            decided_by = $4
        where id = $5
        returning
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        "#,
    )
    .bind(&normalized_status)
    .bind(payload.staff_comments.as_deref())
    .bind(now)
    .bind(&user.id)
    .bind(&feedback_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    Ok(Json(item))
}
