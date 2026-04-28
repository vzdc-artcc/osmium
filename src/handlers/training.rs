use axum::{
    Json,
    extract::{Extension, Path, State},
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
        CreateTrainerReleaseRequestRequest, CreateTrainingAssignmentRequest,
        CreateTrainingAssignmentRequestRequest, DecideTrainerReleaseRequestRequest,
        DecideTrainingAssignmentRequestRequest, TrainerReleaseRequest, TrainingAssignment,
        TrainingAssignmentRequest,
    },
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/api/v1/training/assignments",
    tag = "training",
    responses(
        (status = 200, description = "List assignments", body = [TrainingAssignment]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_assignments(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<TrainingAssignment>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Training, PermissionAction::Update),
    )
    .await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let rows = sqlx::query_as::<_, TrainingAssignment>(
        "select id, student_id, primary_trainer_id, created_at, updated_at from training.training_assignments order by created_at desc",
    )
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(rows))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/assignments",
    tag = "training",
    request_body = CreateTrainingAssignmentRequest,
    responses(
        (status = 201, description = "Assignment created", body = TrainingAssignment),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_assignment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateTrainingAssignmentRequest>,
) -> Result<(StatusCode, Json<TrainingAssignment>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Training, PermissionAction::Update),
    )
    .await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;

    let row = sqlx::query_as::<_, TrainingAssignment>(
        r#"
        insert into training.training_assignments (id, student_id, primary_trainer_id, created_at, updated_at)
        values ($1, $2, $3, $4, $5)
        returning id, student_id, primary_trainer_id, created_at, updated_at
        "#,
    )
    .bind(&id)
    .bind(&payload.student_id)
    .bind(&payload.primary_trainer_id)
    .bind(now)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    if let Some(other_trainer_ids) = payload.other_trainer_ids {
        for trainer_id in other_trainer_ids {
            if trainer_id == payload.primary_trainer_id {
                continue;
            }

            sqlx::query(
                r#"
                insert into training.training_assignment_other_trainers (assignment_id, trainer_id)
                values ($1, $2)
                on conflict (assignment_id, trainer_id) do nothing
                "#,
            )
            .bind(&id)
            .bind(trainer_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::BadRequest)?;
        }
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    get,
    path = "/api/v1/training/assignment-requests",
    tag = "training",
    responses(
        (status = 200, description = "List assignment requests", body = [TrainingAssignmentRequest]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_assignment_requests(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<TrainingAssignmentRequest>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Training, PermissionAction::Update),
    )
    .await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let rows = sqlx::query_as::<_, TrainingAssignmentRequest>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.training_assignment_requests order by submitted_at desc",
    )
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(rows))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/assignment-requests",
    tag = "training",
    request_body = CreateTrainingAssignmentRequestRequest,
    responses(
        (status = 201, description = "Assignment request created", body = TrainingAssignmentRequest),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_assignment_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(_payload): Json<CreateTrainingAssignmentRequestRequest>,
) -> Result<(StatusCode, Json<TrainingAssignmentRequest>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let row = sqlx::query_as::<_, TrainingAssignmentRequest>(
        r#"
        insert into training.training_assignment_requests (id, student_id, submitted_at, status)
        values ($1, $2, $3, 'PENDING')
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(now)
    .fetch_one(db)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/assignment-requests/{request_id}",
    tag = "training",
    params(
        ("request_id" = String, Path, description = "Assignment request ID")
    ),
    request_body = DecideTrainingAssignmentRequestRequest,
    responses(
        (status = 200, description = "Assignment request updated", body = TrainingAssignmentRequest),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn decide_assignment_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(request_id): Path<String>,
    Json(payload): Json<DecideTrainingAssignmentRequestRequest>,
) -> Result<Json<TrainingAssignmentRequest>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Training, PermissionAction::Update),
    )
    .await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "APPROVED" && normalized_status != "DENIED" {
        return Err(ApiError::BadRequest);
    }

    let now = chrono::Utc::now();

    let row = sqlx::query_as::<_, TrainingAssignmentRequest>(
        r#"
        update training.training_assignment_requests
        set status = $1, decided_at = $2, decided_by = $3
        where id = $4
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(&normalized_status)
    .bind(now)
    .bind(&user.id)
    .bind(&request_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    Ok(Json(row))
}

#[utoipa::path(
    get,
    path = "/api/v1/training/trainer-release-requests",
    tag = "training",
    responses(
        (status = 200, description = "List trainer release requests", body = [TrainerReleaseRequest]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_release_requests(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<TrainerReleaseRequest>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Training, PermissionAction::Update),
    )
    .await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let rows = sqlx::query_as::<_, TrainerReleaseRequest>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.trainer_release_requests order by submitted_at desc",
    )
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(rows))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/trainer-release-requests",
    tag = "training",
    request_body = CreateTrainerReleaseRequestRequest,
    responses(
        (status = 201, description = "Trainer release request created", body = TrainerReleaseRequest),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_release_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(_payload): Json<CreateTrainerReleaseRequestRequest>,
) -> Result<(StatusCode, Json<TrainerReleaseRequest>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let row = sqlx::query_as::<_, TrainerReleaseRequest>(
        r#"
        insert into training.trainer_release_requests (id, student_id, submitted_at, status)
        values ($1, $2, $3, 'PENDING')
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(now)
    .fetch_one(db)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/trainer-release-requests/{request_id}",
    tag = "training",
    params(
        ("request_id" = String, Path, description = "Trainer release request ID")
    ),
    request_body = DecideTrainerReleaseRequestRequest,
    responses(
        (status = 200, description = "Trainer release request updated", body = TrainerReleaseRequest),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn decide_release_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(request_id): Path<String>,
    Json(payload): Json<DecideTrainerReleaseRequestRequest>,
) -> Result<Json<TrainerReleaseRequest>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Training, PermissionAction::Update),
    )
    .await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "APPROVED" && normalized_status != "DENIED" {
        return Err(ApiError::BadRequest);
    }

    let now = chrono::Utc::now();

    let row = sqlx::query_as::<_, TrainerReleaseRequest>(
        r#"
        update training.trainer_release_requests
        set status = $1, decided_at = $2, decided_by = $3
        where id = $4
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(&normalized_status)
    .bind(now)
    .bind(&user.id)
    .bind(&request_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    Ok(Json(row))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/assignment-requests/{request_id}/interest",
    tag = "training",
    params(
        ("request_id" = String, Path, description = "Assignment request ID")
    ),
    responses(
        (status = 204, description = "Interest recorded"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn add_assignment_request_interest(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(request_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let exists = sqlx::query_scalar::<_, String>(
        "select id from training.training_assignment_requests where id = $1",
    )
    .bind(&request_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    if exists.is_none() {
        return Err(ApiError::BadRequest);
    }

    sqlx::query(
        r#"
        insert into training.training_assignment_request_interested_trainers (assignment_request_id, trainer_id)
        values ($1, $2)
        on conflict (assignment_request_id, trainer_id) do nothing
        "#,
    )
    .bind(&request_id)
    .bind(&user.id)
    .execute(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/training/assignment-requests/{request_id}/interest",
    tag = "training",
    params(
        ("request_id" = String, Path, description = "Assignment request ID")
    ),
    responses(
        (status = 204, description = "Interest removed"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn remove_assignment_request_interest(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(request_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    sqlx::query(
        "delete from training.training_assignment_request_interested_trainers where assignment_request_id = $1 and trainer_id = $2",
    )
    .bind(&request_id)
    .bind(&user.id)
    .execute(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}
