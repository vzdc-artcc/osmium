use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionKey, PermissionResource},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{
        ApiMessage, CreateOrUpdateTrainingSessionResult, CreateTrainerReleaseRequestRequest,
        CreateTrainingAssignmentRequest, CreateTrainingAssignmentRequestRequest,
        CreateTrainingLessonRequest, CreateTrainingSessionRequest,
        DecideTrainerReleaseRequestRequest, DecideTrainingAssignmentRequestRequest,
        LessonRosterChangeSummary, ListTrainingSessionsQuery, OtsRecommendationSummary,
        TrainerReleaseRequest, TrainingAssignment, TrainingAssignmentRequest, TrainingLesson,
        TrainingSessionDetail, TrainingSessionListItem,
        TrainingSessionPerformanceIndicatorCategoryDetail,
        TrainingSessionPerformanceIndicatorCriteriaDetail,
        TrainingSessionPerformanceIndicatorDetail, TrainingTicketDetail,
        UpdateTrainingLessonRequest, UpdateTrainingSessionRequest,
    },
    repos::audit as audit_repo,
    state::AppState,
};

const TRAINING_PERMISSION: PermissionKey =
    PermissionKey::new(PermissionResource::Training, PermissionAction::Update);

const VALID_PI_MARKERS: &[&str] = &[
    "OBSERVED",
    "NOT_OBSERVED",
    "SATISFACTORY",
    "NEEDS_IMPROVEMENT",
    "UNSATISFACTORY",
];

#[derive(Debug, sqlx::FromRow)]
struct SessionDetailRow {
    id: String,
    student_id: String,
    instructor_id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    additional_comments: Option<String>,
    trainer_comments: Option<String>,
    vatusa_id: Option<String>,
    enable_markdown: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    student_cid: i64,
    student_name: String,
    instructor_cid: i64,
    instructor_name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct TicketRow {
    id: String,
    session_id: String,
    lesson_id: String,
    passed: bool,
    created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ScoreRow {
    id: String,
    training_ticket_id: String,
    criteria_id: String,
    cell_id: String,
    passed: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct IndicatorRootRow {
    id: String,
}

#[derive(Debug, sqlx::FromRow)]
struct IndicatorCategoryRow {
    id: String,
    name: String,
    sort_order: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct IndicatorCriteriaRow {
    id: String,
    category_id: String,
    name: String,
    sort_order: i32,
    marker: Option<String>,
    comments: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LessonRow {
    id: String,
    identifier: String,
    instructor_only: bool,
    notify_instructor_on_pass: bool,
    release_request_on_pass: bool,
    performance_indicator_template_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct RubricMembershipRow {
    lesson_id: String,
    criteria_id: String,
    cell_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExistingTicketRow {
    lesson_id: String,
    passed: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct MembershipRow {
    controller_status: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct UserIdentityRow {
    id: String,
    cid: i64,
    full_name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SessionExistsRow {
    id: String,
    instructor_id: String,
}

#[derive(Debug, Clone)]
struct RubricRule {
    criteria_ids: HashSet<String>,
    cells_by_criteria: HashMap<String, HashSet<String>>,
}

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
    ensure_training_permission(&state, user).await?;
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
    headers: HeaderMap,
    Json(payload): Json<CreateTrainingAssignmentRequest>,
) -> Result<(StatusCode, Json<TrainingAssignment>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
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

    let actor = audit_repo::resolve_audit_actor(&mut *tx, Some(user), None).await?;
    record_audit(
        &mut tx,
        actor.actor_id.as_deref(),
        "CREATE",
        "TRAINING_ASSIGNMENT",
        Some(&row.id),
        "training_session",
        Some(&row.id),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    get,
    path = "/api/v1/training/lessons",
    tag = "training",
    responses(
        (status = 200, description = "List training lessons", body = [TrainingLesson]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_lessons(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<TrainingLesson>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let rows = sqlx::query_as::<_, TrainingLesson>(
        r#"
        select
            id,
            identifier,
            location,
            name,
            description,
            position,
            facility,
            rubric_id,
            updated_at,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            duration,
            trainee_preparation,
            performance_indicator_template_id,
            created_at
        from training.lessons
        order by location asc, identifier asc, name asc
        "#,
    )
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(rows))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/lessons",
    tag = "training",
    request_body = CreateTrainingLessonRequest,
    responses(
        (status = 201, description = "Training lesson created", body = TrainingLesson),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_lesson(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateTrainingLessonRequest>,
) -> Result<(StatusCode, Json<TrainingLesson>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    validate_training_lesson_payload(&payload.identifier, payload.location, payload.duration)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;
    let now = Utc::now();
    let lesson_id = Uuid::new_v4().to_string();

    let row = sqlx::query_as::<_, TrainingLesson>(
        r#"
        insert into training.lessons (
            id,
            identifier,
            location,
            name,
            description,
            position,
            facility,
            updated_at,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            duration,
            trainee_preparation,
            performance_indicator_template_id,
            created_at
        )
        values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $8
        )
        returning
            id,
            identifier,
            location,
            name,
            description,
            position,
            facility,
            rubric_id,
            updated_at,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            duration,
            trainee_preparation,
            performance_indicator_template_id,
            created_at
        "#,
    )
    .bind(&lesson_id)
    .bind(payload.identifier.trim())
    .bind(payload.location)
    .bind(payload.name.trim())
    .bind(payload.description.trim())
    .bind(payload.position.trim())
    .bind(payload.facility.trim())
    .bind(now)
    .bind(payload.instructor_only)
    .bind(payload.notify_instructor_on_pass)
    .bind(payload.release_request_on_pass)
    .bind(payload.duration)
    .bind(payload.trainee_preparation.as_deref())
    .bind(payload.performance_indicator_template_id.as_deref())
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "CREATE",
        "LESSON",
        Some(&row.id),
        "training_session",
        Some(&row.id),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/lessons/{lesson_id}",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID")
    ),
    request_body = UpdateTrainingLessonRequest,
    responses(
        (status = 200, description = "Training lesson updated", body = TrainingLesson),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_lesson(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateTrainingLessonRequest>,
) -> Result<Json<TrainingLesson>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    validate_training_lesson_payload(&payload.identifier, payload.location, payload.duration)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;
    let now = Utc::now();
    let before = sqlx::query_as::<_, TrainingLesson>(
        r#"
        select
            id, identifier, location, name, description, position, facility, rubric_id, updated_at,
            instructor_only, notify_instructor_on_pass, release_request_on_pass, duration,
            trainee_preparation, performance_indicator_template_id, created_at
        from training.lessons
        where id = $1
        "#,
    )
    .bind(&lesson_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let row = sqlx::query_as::<_, TrainingLesson>(
        r#"
        update training.lessons
        set
            identifier = $2,
            location = $3,
            name = $4,
            description = $5,
            position = $6,
            facility = $7,
            updated_at = $8,
            instructor_only = $9,
            notify_instructor_on_pass = $10,
            release_request_on_pass = $11,
            duration = $12,
            trainee_preparation = $13,
            performance_indicator_template_id = $14
        where id = $1
        returning
            id,
            identifier,
            location,
            name,
            description,
            position,
            facility,
            rubric_id,
            updated_at,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            duration,
            trainee_preparation,
            performance_indicator_template_id,
            created_at
        "#,
    )
    .bind(&lesson_id)
    .bind(payload.identifier.trim())
    .bind(payload.location)
    .bind(payload.name.trim())
    .bind(payload.description.trim())
    .bind(payload.position.trim())
    .bind(payload.facility.trim())
    .bind(now)
    .bind(payload.instructor_only)
    .bind(payload.notify_instructor_on_pass)
    .bind(payload.release_request_on_pass)
    .bind(payload.duration)
    .bind(payload.trainee_preparation.as_deref())
    .bind(payload.performance_indicator_template_id.as_deref())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "UPDATE",
        "LESSON",
        Some(&row.id),
        "training_session",
        Some(&row.id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(row))
}

#[utoipa::path(
    delete,
    path = "/api/v1/training/lessons/{lesson_id}",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID")
    ),
    responses(
        (status = 204, description = "Training lesson deleted"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn delete_lesson(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let deleted = sqlx::query_as::<_, TrainingLesson>(
        r#"
        delete from training.lessons
        where id = $1
        returning
            id,
            identifier,
            location,
            name,
            description,
            position,
            facility,
            rubric_id,
            updated_at,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            duration,
            trainee_preparation,
            performance_indicator_template_id,
            created_at
        "#,
    )
    .bind(&lesson_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::BadRequest)?
    .ok_or(ApiError::BadRequest)?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "DELETE",
        "LESSON",
        Some(&deleted.id),
        "training_session",
        Some(&deleted.id),
        Some(audit_repo::sanitized_snapshot(&deleted)?),
        None,
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
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
    ensure_training_permission(&state, user).await?;
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
    headers: HeaderMap,
    Json(_payload): Json<CreateTrainingAssignmentRequestRequest>,
) -> Result<(StatusCode, Json<TrainingAssignmentRequest>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "CREATE".to_string(),
            resource_type: "TRAINING_ASSIGNMENT_REQUEST".to_string(),
            resource_id: Some(row.id.clone()),
            scope_type: "training_session".to_string(),
            scope_key: Some(row.student_id.clone()),
            before_state: None,
            after_state: Some(audit_repo::sanitized_snapshot(&row)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
    Json(payload): Json<DecideTrainingAssignmentRequestRequest>,
) -> Result<Json<TrainingAssignmentRequest>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "APPROVED" && normalized_status != "DENIED" {
        return Err(ApiError::BadRequest);
    }

    let now = Utc::now();
    let before = sqlx::query_as::<_, TrainingAssignmentRequest>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.training_assignment_requests where id = $1",
    )
    .bind(&request_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "DECIDE".to_string(),
            resource_type: "TRAINING_ASSIGNMENT_REQUEST".to_string(),
            resource_id: Some(row.id.clone()),
            scope_type: "training_session".to_string(),
            scope_key: Some(row.student_id.clone()),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&row)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    ensure_training_permission(&state, user).await?;
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
    headers: HeaderMap,
    Json(_payload): Json<CreateTrainerReleaseRequestRequest>,
) -> Result<(StatusCode, Json<TrainerReleaseRequest>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "CREATE".to_string(),
            resource_type: "TRAINER_RELEASE_REQUEST".to_string(),
            resource_id: Some(row.id.clone()),
            scope_type: "training_session".to_string(),
            scope_key: Some(row.student_id.clone()),
            before_state: None,
            after_state: Some(audit_repo::sanitized_snapshot(&row)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
    Json(payload): Json<DecideTrainerReleaseRequestRequest>,
) -> Result<Json<TrainerReleaseRequest>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "APPROVED" && normalized_status != "DENIED" {
        return Err(ApiError::BadRequest);
    }

    let now = Utc::now();
    let before = sqlx::query_as::<_, TrainerReleaseRequest>(
        "select id, student_id, submitted_at, status, decided_at, decided_by from training.trainer_release_requests where id = $1",
    )
    .bind(&request_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "DECIDE".to_string(),
            resource_type: "TRAINER_RELEASE_REQUEST".to_string(),
            resource_id: Some(row.id.clone()),
            scope_type: "training_session".to_string(),
            scope_key: Some(row.student_id.clone()),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&row)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "ASSIGN".to_string(),
            resource_type: "TRAINING_ASSIGNMENT_REQUEST_INTEREST".to_string(),
            resource_id: Some(request_id.clone()),
            scope_type: "training_session".to_string(),
            scope_key: Some(request_id),
            before_state: None,
            after_state: Some(serde_json::json!({ "trainer_id": user.id })),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

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
    headers: HeaderMap,
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

    let actor = audit_repo::resolve_audit_actor(db, Some(user), None).await?;
    audit_repo::record_audit(
        db,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UNASSIGN".to_string(),
            resource_type: "TRAINING_ASSIGNMENT_REQUEST_INTEREST".to_string(),
            resource_id: Some(request_id.clone()),
            scope_type: "training_session".to_string(),
            scope_key: Some(request_id),
            before_state: Some(serde_json::json!({ "trainer_id": user.id })),
            after_state: None,
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/training/sessions",
    tag = "training",
    params(ListTrainingSessionsQuery),
    responses(
        (status = 200, description = "List training sessions", body = [TrainingSessionListItem]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_training_sessions(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListTrainingSessionsQuery>,
) -> Result<Json<Vec<TrainingSessionListItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let sort_column = match query.sort_field.as_deref() {
        Some("end") => "ts.end",
        _ => "ts.start",
    };
    let sort_direction = match query.sort_order.as_deref() {
        Some(value) if value.eq_ignore_ascii_case("asc") => "asc",
        _ => "desc",
    };
    let filter_field = query.filter_field.clone().unwrap_or_default();
    let filter_value = query.filter_value.clone().unwrap_or_default();
    let filter_is_exact = query
        .filter_operator
        .as_deref()
        .map(normalize_filter_mode)
        .is_some_and(|mode| mode == FilterMode::Exact);
    let filter_pattern = query
        .filter_operator
        .as_deref()
        .map(|op| build_filter_pattern(op, &filter_value))
        .unwrap_or_else(|| filter_value.clone());

    let count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(distinct ts.id)::bigint
        from training.training_sessions ts
        join identity.users su on su.id = ts.student_id
        join identity.users iu on iu.id = ts.instructor_id
        left join training.training_tickets tt on tt.session_id = ts.id
        left join training.lessons l on l.id = tt.lesson_id
        where ($1::text is null or ts.student_id = $1)
          and ($2::text is null or ts.instructor_id = $2)
          and (
            $3::text = ''
            or (
                $3 = 'student'
                and (
                    ($5 and (cast(su.cid as text) = $4 or su.full_name = $4))
                    or
                    (not $5 and (cast(su.cid as text) ilike $4 or su.full_name ilike $4))
                )
            )
            or (
                $3 = 'instructor'
                and (
                    ($5 and (cast(iu.cid as text) = $4 or iu.full_name = $4))
                    or
                    (not $5 and (cast(iu.cid as text) ilike $4 or iu.full_name ilike $4))
                )
            )
            or (
                $3 = 'lessons'
                and (
                    ($5 and (l.identifier = $4 or l.name = $4))
                    or
                    (not $5 and (l.identifier ilike $4 or l.name ilike $4))
                )
            )
          )
        "#,
    )
    .bind(query.student_id.as_deref())
    .bind(query.instructor_id.as_deref())
    .bind(&filter_field)
    .bind(&filter_pattern)
    .bind(filter_is_exact)
    .fetch_one(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let sql = format!(
        r#"
        select
            ts.id,
            ts.student_id,
            ts.instructor_id,
            ts.start,
            ts."end" as "end",
            ts.additional_comments,
            ts.trainer_comments,
            ts.vatusa_id,
            ts.enable_markdown,
            ts.created_at,
            ts.updated_at,
            su.cid as student_cid,
            su.full_name as student_name,
            iu.cid as instructor_cid,
            iu.full_name as instructor_name,
            count(tt.id)::bigint as ticket_count
        from training.training_sessions ts
        join identity.users su on su.id = ts.student_id
        join identity.users iu on iu.id = ts.instructor_id
        left join training.training_tickets tt on tt.session_id = ts.id
        left join training.lessons l on l.id = tt.lesson_id
        where ($1::text is null or ts.student_id = $1)
          and ($2::text is null or ts.instructor_id = $2)
          and (
            $3::text = ''
            or (
                $3 = 'student'
                and (
                    ($5 and (cast(su.cid as text) = $4 or su.full_name = $4))
                    or
                    (not $5 and (cast(su.cid as text) ilike $4 or su.full_name ilike $4))
                )
            )
            or (
                $3 = 'instructor'
                and (
                    ($5 and (cast(iu.cid as text) = $4 or iu.full_name = $4))
                    or
                    (not $5 and (cast(iu.cid as text) ilike $4 or iu.full_name ilike $4))
                )
            )
            or (
                $3 = 'lessons'
                and (
                    ($5 and (l.identifier = $4 or l.name = $4))
                    or
                    (not $5 and (l.identifier ilike $4 or l.name ilike $4))
                )
            )
          )
        group by
            ts.id, su.cid, su.full_name, iu.cid, iu.full_name
        order by {sort_column} {sort_direction}
        limit $6 offset $7
        "#
    );

    let mut items = sqlx::query_as::<_, TrainingSessionListItem>(&sql)
        .bind(query.student_id.as_deref())
        .bind(query.instructor_id.as_deref())
        .bind(&filter_field)
        .bind(&filter_pattern)
        .bind(filter_is_exact)
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await
        .map_err(|_| ApiError::Internal)?;

    if count == 0 {
        items.clear();
    }

    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/api/v1/training/sessions/{session_id}",
    tag = "training",
    params(
        ("session_id" = String, Path, description = "Training session ID")
    ),
    responses(
        (status = 200, description = "Training session detail", body = TrainingSessionDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_training_session(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(session_id): Path<String>,
) -> Result<Json<TrainingSessionDetail>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let detail = fetch_training_session_detail(db, &session_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    Ok(Json(detail))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/sessions",
    tag = "training",
    request_body = CreateTrainingSessionRequest,
    responses(
        (status = 201, description = "Training session created", body = CreateOrUpdateTrainingSessionResult),
        (status = 400, description = "Invalid request", body = CreateOrUpdateTrainingSessionResult),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_training_session(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateTrainingSessionRequest>,
) -> Result<(StatusCode, Json<CreateOrUpdateTrainingSessionResult>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    match upsert_training_session(db, user, None, payload.into_update_request()).await? {
        Ok(result) => Ok((StatusCode::CREATED, Json(result))),
        Err(errors) => Ok((StatusCode::BAD_REQUEST, Json(error_result(errors)))),
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/sessions/{session_id}",
    tag = "training",
    params(
        ("session_id" = String, Path, description = "Training session ID")
    ),
    request_body = UpdateTrainingSessionRequest,
    responses(
        (status = 200, description = "Training session updated", body = CreateOrUpdateTrainingSessionResult),
        (status = 400, description = "Invalid request", body = CreateOrUpdateTrainingSessionResult),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_training_session(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(session_id): Path<String>,
    Json(payload): Json<UpdateTrainingSessionRequest>,
) -> Result<Json<CreateOrUpdateTrainingSessionResult>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    match upsert_training_session(db, user, Some(session_id), payload).await? {
        Ok(result) => Ok(Json(result)),
        Err(_) => Err(ApiError::BadRequest),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/training/sessions/{session_id}",
    tag = "training",
    params(
        ("session_id" = String, Path, description = "Training session ID")
    ),
    responses(
        (status = 204, description = "Training session deleted"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn delete_training_session(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_permission(&state, user).await?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let deleted = sqlx::query_as::<_, SessionExistsRow>(
        r#"
        delete from training.training_sessions
        where id = $1
        returning id, instructor_id
        "#,
    )
    .bind(&session_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "DELETE",
        "TRAINING_SESSION",
        Some(&deleted.id),
        "training_session",
        Some(&deleted.id),
        Some(serde_json::json!({ "id": deleted.id, "instructor_id": deleted.instructor_id })),
        None,
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_training_permission(state: &AppState, user: &CurrentUser) -> Result<(), ApiError> {
    ensure_permission(state, Some(user), None, TRAINING_PERMISSION).await
}

fn validate_training_lesson_payload(
    identifier: &str,
    location: i32,
    duration: i32,
) -> Result<(), ApiError> {
    if identifier.trim().is_empty()
        || location < 0
        || location > 2
        || duration < 10
        || duration > 12 * 60
    {
        return Err(ApiError::BadRequest);
    }

    Ok(())
}

async fn fetch_training_session_detail(
    db: &sqlx::PgPool,
    session_id: &str,
) -> Result<Option<TrainingSessionDetail>, ApiError> {
    let session = sqlx::query_as::<_, SessionDetailRow>(
        r#"
        select
            ts.id,
            ts.student_id,
            ts.instructor_id,
            ts.start,
            ts."end" as "end",
            ts.additional_comments,
            ts.trainer_comments,
            ts.vatusa_id,
            ts.enable_markdown,
            ts.created_at,
            ts.updated_at,
            su.cid as student_cid,
            su.full_name as student_name,
            iu.cid as instructor_cid,
            iu.full_name as instructor_name
        from training.training_sessions ts
        join identity.users su on su.id = ts.student_id
        join identity.users iu on iu.id = ts.instructor_id
        where ts.id = $1
        "#,
    )
    .bind(session_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let Some(session) = session else {
        return Ok(None);
    };

    let ticket_rows = sqlx::query_as::<_, TicketRow>(
        r#"
        select id, session_id, lesson_id, passed, created_at
        from training.training_tickets
        where session_id = $1
        order by created_at asc, id asc
        "#,
    )
    .bind(session_id)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let score_rows = sqlx::query_as::<_, ScoreRow>(
        r#"
        select id, training_ticket_id, criteria_id, cell_id, passed
        from training.rubric_scores
        where training_ticket_id in (
            select id from training.training_tickets where session_id = $1
        )
        order by id asc
        "#,
    )
    .bind(session_id)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut scores_by_ticket: HashMap<String, Vec<_>> = HashMap::new();
    for row in score_rows {
        scores_by_ticket
            .entry(row.training_ticket_id)
            .or_default()
            .push(crate::models::RubricScoreDetail {
                id: row.id,
                criteria_id: row.criteria_id,
                cell_id: row.cell_id,
                passed: row.passed,
            });
    }

    let tickets = ticket_rows
        .into_iter()
        .map(|row| TrainingTicketDetail {
            id: row.id.clone(),
            session_id: row.session_id,
            lesson_id: row.lesson_id,
            passed: row.passed,
            created_at: row.created_at,
            scores: scores_by_ticket.remove(&row.id).unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    let performance_indicator = fetch_session_performance_indicator(db, session_id).await?;

    Ok(Some(TrainingSessionDetail {
        id: session.id,
        student_id: session.student_id,
        instructor_id: session.instructor_id,
        start: session.start,
        end: session.end,
        additional_comments: session.additional_comments,
        trainer_comments: session.trainer_comments,
        vatusa_id: session.vatusa_id,
        enable_markdown: session.enable_markdown,
        created_at: session.created_at,
        updated_at: session.updated_at,
        student_cid: session.student_cid,
        student_name: session.student_name,
        instructor_cid: session.instructor_cid,
        instructor_name: session.instructor_name,
        tickets,
        performance_indicator,
    }))
}

async fn fetch_session_performance_indicator(
    db: &sqlx::PgPool,
    session_id: &str,
) -> Result<Option<TrainingSessionPerformanceIndicatorDetail>, ApiError> {
    let root = sqlx::query_as::<_, IndicatorRootRow>(
        "select id from training.session_performance_indicators where training_session_id = $1",
    )
    .bind(session_id)
    .fetch_optional(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let Some(root) = root else {
        return Ok(None);
    };

    let category_rows = sqlx::query_as::<_, IndicatorCategoryRow>(
        r#"
        select id, name, sort_order
        from training.session_performance_indicator_categories
        where session_performance_indicator_id = $1
        order by sort_order asc, id asc
        "#,
    )
    .bind(&root.id)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let criteria_rows = sqlx::query_as::<_, IndicatorCriteriaRow>(
        r#"
        select id, category_id, name, sort_order, marker, comments
        from training.session_performance_indicator_criteria
        where category_id in (
            select id
            from training.session_performance_indicator_categories
            where session_performance_indicator_id = $1
        )
        order by sort_order asc, id asc
        "#,
    )
    .bind(&root.id)
    .fetch_all(db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut criteria_by_category: HashMap<
        String,
        Vec<TrainingSessionPerformanceIndicatorCriteriaDetail>,
    > = HashMap::new();
    for row in criteria_rows {
        criteria_by_category
            .entry(row.category_id)
            .or_default()
            .push(TrainingSessionPerformanceIndicatorCriteriaDetail {
                id: row.id,
                name: row.name,
                order: row.sort_order,
                marker: row.marker,
                comments: row.comments,
            });
    }

    let categories = category_rows
        .into_iter()
        .map(|row| TrainingSessionPerformanceIndicatorCategoryDetail {
            id: row.id.clone(),
            name: row.name,
            order: row.sort_order,
            criteria: criteria_by_category.remove(&row.id).unwrap_or_default(),
        })
        .collect();

    Ok(Some(TrainingSessionPerformanceIndicatorDetail {
        id: root.id,
        categories,
    }))
}

async fn upsert_training_session(
    db: &sqlx::PgPool,
    user: &CurrentUser,
    session_id: Option<String>,
    payload: UpdateTrainingSessionRequest,
) -> Result<Result<CreateOrUpdateTrainingSessionResult, Vec<ApiMessage>>, ApiError> {
    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let student = sqlx::query_as::<_, UserIdentityRow>(
        "select id, cid, full_name from identity.users where id = $1",
    )
    .bind(&payload.student_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let Some(student) = student else {
        return Ok(Err(vec![message("Student does not exist.")]));
    };

    let lessons = sqlx::query_as::<_, LessonRow>(
        r#"
        select
            id,
            identifier,
            instructor_only,
            notify_instructor_on_pass,
            release_request_on_pass,
            performance_indicator_template_id
        from training.lessons
        where id = any($1)
        "#,
    )
    .bind(payload.ticket_lesson_ids())
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let lesson_map = lessons
        .into_iter()
        .map(|lesson| (lesson.id.clone(), lesson))
        .collect::<HashMap<_, _>>();

    let rubric_rows = sqlx::query_as::<_, RubricMembershipRow>(
        r#"
        select
            l.id as lesson_id,
            c.id as criteria_id,
            cell.id as cell_id
        from training.lessons l
        join training.lesson_rubrics r on r.id = l.rubric_id
        join training.lesson_rubric_criteria c on c.rubric_id = r.id
        left join training.lesson_rubric_cells cell on cell.criteria_id = c.id
        where l.id = any($1)
        "#,
    )
    .bind(payload.ticket_lesson_ids())
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let rules = build_rubric_rules(rubric_rows);
    let validation_errors = validate_training_session_payload(&payload, &lesson_map, &rules);
    if !validation_errors.is_empty() {
        return Ok(Err(validation_errors));
    }

    let membership = sqlx::query_as::<_, MembershipRow>(
        "select controller_status from org.memberships where user_id = $1",
    )
    .bind(&student.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let now = Utc::now();
    let existing_id = session_id.clone();
    let (session_id, _instructor_id, old_tickets) = if let Some(ref id) = existing_id {
        let existing = sqlx::query_as::<_, SessionExistsRow>(
            "select id, instructor_id from training.training_sessions where id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::BadRequest)?;

        let old_tickets = sqlx::query_as::<_, ExistingTicketRow>(
            r#"
            select lesson_id, passed
            from training.training_tickets
            where session_id = $1
            "#,
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        sqlx::query(
            "delete from training.session_performance_indicators where training_session_id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        sqlx::query("delete from training.training_tickets where session_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;

        sqlx::query(
            r#"
            update training.training_sessions
            set student_id = $2,
                start = $3,
                "end" = $4,
                additional_comments = $5,
                trainer_comments = $6,
                enable_markdown = $7,
                updated_at = $8
            where id = $1
            "#,
        )
        .bind(id)
        .bind(&payload.student_id)
        .bind(payload.start)
        .bind(payload.end)
        .bind(payload.additional_comments.as_deref())
        .bind(payload.trainer_comments.as_deref())
        .bind(payload.enable_markdown.unwrap_or(false))
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        (existing.id, existing.instructor_id, old_tickets)
    } else {
        let new_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            insert into training.training_sessions (
                id,
                student_id,
                instructor_id,
                start,
                "end",
                additional_comments,
                trainer_comments,
                enable_markdown,
                created_at,
                updated_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            "#,
        )
        .bind(&new_id)
        .bind(&payload.student_id)
        .bind(&user.id)
        .bind(payload.start)
        .bind(payload.end)
        .bind(payload.additional_comments.as_deref())
        .bind(payload.trainer_comments.as_deref())
        .bind(payload.enable_markdown.unwrap_or(false))
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        (new_id, user.id.clone(), Vec::new())
    };

    let mut new_passed_lessons = Vec::new();
    for ticket in &payload.tickets {
        let ticket_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            insert into training.training_tickets (id, session_id, lesson_id, passed, created_at)
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&ticket_id)
        .bind(&session_id)
        .bind(&ticket.lesson_id)
        .bind(ticket.passed)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        for score in &ticket.scores {
            sqlx::query(
                r#"
                insert into training.rubric_scores (
                    id,
                    training_ticket_id,
                    criteria_id,
                    cell_id,
                    passed
                )
                values ($1, $2, $3, $4, $5)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&ticket_id)
            .bind(&score.criteria_id)
            .bind(&score.cell_id)
            .bind(score.passed)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;
        }

        if ticket.passed {
            if let Some(lesson) = lesson_map.get(&ticket.lesson_id) {
                new_passed_lessons.push(lesson.clone());
            }
        }
    }

    if let Some(ref indicator) = payload.performance_indicator {
        let indicator_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            insert into training.session_performance_indicators (id, training_session_id, created_at)
            values ($1, $2, $3)
            "#,
        )
        .bind(&indicator_id)
        .bind(&session_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        for category in &indicator.categories {
            let category_id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                insert into training.session_performance_indicator_categories (
                    id,
                    session_performance_indicator_id,
                    name,
                    sort_order
                )
                values ($1, $2, $3, $4)
                "#,
            )
            .bind(&category_id)
            .bind(&indicator_id)
            .bind(&category.name)
            .bind(category.order)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;

            for criteria in &category.criteria {
                sqlx::query(
                    r#"
                    insert into training.session_performance_indicator_criteria (
                        id,
                        category_id,
                        name,
                        sort_order,
                        marker,
                        comments
                    )
                    values ($1, $2, $3, $4, $5, $6)
                    "#,
                )
                .bind(Uuid::new_v4().to_string())
                .bind(&category_id)
                .bind(&criteria.name)
                .bind(criteria.order)
                .bind(criteria.marker.trim().to_ascii_uppercase())
                .bind(criteria.comments.as_deref())
                .execute(&mut *tx)
                .await
                .map_err(|_| ApiError::Internal)?;
            }
        }
    }

    let old_passed_lesson_ids = old_tickets
        .iter()
        .filter(|ticket| ticket.passed)
        .map(|ticket| ticket.lesson_id.clone())
        .collect::<HashSet<_>>();
    let new_passed_lesson_ids = new_passed_lessons
        .iter()
        .map(|lesson| lesson.id.clone())
        .collect::<HashSet<_>>();

    let roster_updates = apply_roster_changes(
        &mut tx,
        actor_id.as_deref(),
        user.id.as_str(),
        &student.id,
        student.cid,
        &new_passed_lesson_ids,
        &old_passed_lesson_ids,
    )
    .await?;

    let release = maybe_create_release_request(
        &mut tx,
        &student.id,
        membership.and_then(|row| row.controller_status),
        &new_passed_lessons,
        actor_id.as_deref(),
    )
    .await?;

    let ots_recommendation = sync_ots_recommendations(
        &mut tx,
        actor_id.as_deref(),
        user.id.as_str(),
        &student.id,
        &student.full_name,
        &new_passed_lessons,
        &old_passed_lesson_ids,
        payload.start,
    )
    .await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        if existing_id.is_some() {
            "UPDATE"
        } else {
            "CREATE"
        },
        "TRAINING_SESSION",
        Some(&session_id),
        "training_session",
        Some(&session_id),
        None,
        None,
        None,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let session = fetch_training_session_detail(db, &session_id)
        .await?
        .ok_or(ApiError::Internal)?;

    Ok(Ok(CreateOrUpdateTrainingSessionResult {
        session: Some(session),
        release,
        roster_updates,
        ots_recommendation,
        errors: Vec::new(),
    }))
}

fn build_rubric_rules(rows: Vec<RubricMembershipRow>) -> HashMap<String, RubricRule> {
    let mut rules = HashMap::<String, RubricRule>::new();

    for row in rows {
        let entry = rules.entry(row.lesson_id).or_insert_with(|| RubricRule {
            criteria_ids: HashSet::new(),
            cells_by_criteria: HashMap::new(),
        });
        entry.criteria_ids.insert(row.criteria_id.clone());
        if let Some(cell_id) = row.cell_id {
            entry
                .cells_by_criteria
                .entry(row.criteria_id)
                .or_default()
                .insert(cell_id);
        }
    }

    rules
}

fn validate_training_session_payload(
    payload: &UpdateTrainingSessionRequest,
    lesson_map: &HashMap<String, LessonRow>,
    rules: &HashMap<String, RubricRule>,
) -> Vec<ApiMessage> {
    let mut errors = Vec::new();

    if payload.student_id.trim().is_empty() {
        errors.push(message("You must select a student."));
    }

    let duration = payload
        .end
        .signed_duration_since(payload.start)
        .num_minutes();
    if duration < 5 || duration > 12 * 60 {
        errors.push(message(
            "Session must be between 5 minutes and 12 hours long.",
        ));
    }

    if payload.tickets.is_empty() {
        errors.push(message("You must add at least one training ticket."));
        return errors;
    }

    for ticket in &payload.tickets {
        let Some(lesson) = lesson_map.get(&ticket.lesson_id) else {
            errors.push(message("One or more lessons do not exist."));
            continue;
        };

        let Some(rule) = rules.get(&lesson.id) else {
            if !ticket.scores.is_empty() {
                errors.push(message(
                    "Rubric scores are only allowed for lessons with a rubric.",
                ));
            }
            continue;
        };

        if ticket.scores.len() != rule.criteria_ids.len() {
            errors.push(message(
                "Rubric-backed lessons require exactly one score per rubric criteria.",
            ));
            continue;
        }

        let mut seen_criteria = HashSet::new();
        for score in &ticket.scores {
            if !rule.criteria_ids.contains(&score.criteria_id) {
                errors.push(message(
                    "A submitted rubric criteria does not belong to the lesson.",
                ));
                continue;
            }
            if !seen_criteria.insert(score.criteria_id.clone()) {
                errors.push(message("Duplicate rubric criteria scores are not allowed."));
                continue;
            }
            let valid_cells = rule.cells_by_criteria.get(&score.criteria_id);
            if !valid_cells.is_some_and(|cells| cells.contains(&score.cell_id)) {
                errors.push(message(
                    "A submitted rubric cell does not belong to the specified criteria.",
                ));
            }
        }
    }

    if let Some(first_lesson_id) = payload
        .tickets
        .first()
        .map(|ticket| ticket.lesson_id.as_str())
    {
        if let Some(first_lesson) = lesson_map.get(first_lesson_id) {
            match (
                &first_lesson.performance_indicator_template_id,
                &payload.performance_indicator,
            ) {
                (Some(_), None) => {
                    errors.push(message(
                        "You must fill out all performance indicators to submit this ticket.",
                    ));
                }
                (Some(_), Some(indicator)) => {
                    let markers_complete = indicator.categories.iter().all(|category| {
                        category.criteria.iter().all(|criteria| {
                            let marker = criteria.marker.trim().to_ascii_uppercase();
                            !marker.is_empty() && VALID_PI_MARKERS.contains(&marker.as_str())
                        })
                    });
                    if !markers_complete {
                        errors.push(message(
                            "You must fill out all performance indicators to submit this ticket.",
                        ));
                    }
                }
                (None, Some(_)) => {
                    errors.push(message(
                        "Performance indicators are not allowed for the first lesson in this session.",
                    ));
                }
                (None, None) => {}
            }
        }
    }

    errors
}

async fn apply_roster_changes(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Option<&str>,
    writer_user_id: &str,
    student_user_id: &str,
    student_cid: i64,
    new_passed_lesson_ids: &HashSet<String>,
    old_passed_lesson_ids: &HashSet<String>,
) -> Result<Vec<LessonRosterChangeSummary>, ApiError> {
    let lesson_ids = new_passed_lesson_ids
        .difference(old_passed_lesson_ids)
        .cloned()
        .collect::<Vec<_>>();
    if lesson_ids.is_empty() {
        return Ok(Vec::new());
    }

    let updates = sqlx::query_as::<_, LessonRosterChangeSummary>(
        r#"
        select
            id,
            lesson_id,
            certification_type_id,
            certification_option,
            dossier_text
        from training.lesson_roster_changes
        where lesson_id = any($1)
        "#,
    )
    .bind(&lesson_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let now = Utc::now();
    for update in &updates {
        sqlx::query(
            "delete from org.user_solo_certifications where user_id = $1 and certification_type_id = $2",
        )
        .bind(student_user_id)
        .bind(&update.certification_type_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        sqlx::query(
            r#"
            insert into org.user_certifications (
                id,
                user_id,
                certification_type_id,
                certification_option,
                granted_at,
                granted_by_actor_id
            )
            values ($1, $2, $3, $4, $5, $6)
            on conflict (user_id, certification_type_id) do update
            set certification_option = excluded.certification_option,
                granted_by_actor_id = excluded.granted_by_actor_id
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(student_user_id)
        .bind(&update.certification_type_id)
        .bind(&update.certification_option)
        .bind(now)
        .bind(actor_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        sqlx::query(
            r#"
            insert into feedback.dossier_entries (id, user_id, writer_id, message, timestamp, created_at)
            values ($1, $2, $3, $4, $5, $5)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(student_user_id)
        .bind(writer_user_id)
        .bind(
            update
                .dossier_text
                .replace("{cid}", &student_cid.to_string()),
        )
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    Ok(updates)
}

async fn maybe_create_release_request(
    tx: &mut Transaction<'_, Postgres>,
    student_user_id: &str,
    controller_status: Option<String>,
    passed_lessons: &[LessonRow],
    actor_id: Option<&str>,
) -> Result<Option<TrainerReleaseRequest>, ApiError> {
    if controller_status.as_deref() != Some("HOME")
        || !passed_lessons
            .iter()
            .any(|lesson| lesson.release_request_on_pass)
    {
        return Ok(None);
    }

    let assignment = sqlx::query_scalar::<_, String>(
        "select id from training.training_assignments where student_id = $1",
    )
    .bind(student_user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    if assignment.is_none() {
        return Ok(None);
    }

    let existing = sqlx::query_scalar::<_, String>(
        "select id from training.trainer_release_requests where student_id = $1",
    )
    .bind(student_user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    if existing.is_some() {
        return Ok(None);
    }

    let now = Utc::now();
    let row = sqlx::query_as::<_, TrainerReleaseRequest>(
        r#"
        insert into training.trainer_release_requests (id, student_id, submitted_at, status, created_at, updated_at)
        values ($1, $2, $3, 'PENDING', $3, $3)
        returning id, student_id, submitted_at, status, decided_at, decided_by
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(student_user_id)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    record_audit(
        tx,
        actor_id,
        "CREATE",
        "TRAINER_RELEASE_REQUEST",
        Some(&row.id),
        "training_session",
        Some(student_user_id),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
        None,
    )
    .await?;

    Ok(Some(row))
}

async fn sync_ots_recommendations(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Option<&str>,
    trainer_user_id: &str,
    student_user_id: &str,
    trainer_name: &str,
    passed_lessons: &[LessonRow],
    old_passed_lesson_ids: &HashSet<String>,
    start: DateTime<Utc>,
) -> Result<Option<OtsRecommendationSummary>, ApiError> {
    if passed_lessons.iter().any(|lesson| lesson.instructor_only) {
        let deleted_ids = sqlx::query_scalar::<_, String>(
            "delete from training.ots_recommendations where student_id = $1 returning id",
        )
        .bind(student_user_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        for deleted_id in deleted_ids {
            record_audit(
                tx,
                actor_id,
                "DELETE",
                "OTS_RECOMMENDATION",
                Some(&deleted_id),
                "training_session",
                Some(student_user_id),
                Some(serde_json::json!({ "id": deleted_id })),
                None,
                None,
            )
            .await?;
        }
    }

    for lesson in passed_lessons {
        if !lesson.notify_instructor_on_pass || old_passed_lesson_ids.contains(&lesson.id) {
            continue;
        }

        let now = Utc::now();
        let note = format!(
            "{} w/ {} ON {}.",
            lesson.identifier,
            trainer_name,
            format_zulu(start)
        );

        let rec = sqlx::query_as::<_, OtsRecommendationSummary>(
            r#"
            insert into training.ots_recommendations (
                id,
                student_id,
                assigned_instructor_id,
                notes,
                created_at,
                updated_at
            )
            values ($1, $2, null, $3, $4, $4)
            returning id, student_id, assigned_instructor_id, notes, created_at, updated_at
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(student_user_id)
        .bind(note)
        .bind(now)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;

        record_audit(
            tx,
            actor_id,
            "CREATE",
            "OTS_RECOMMENDATION",
            Some(&rec.id),
            "training_session",
            Some(trainer_user_id),
            None,
            Some(audit_repo::sanitized_snapshot(&rec)?),
            None,
        )
        .await?;

        return Ok(Some(rec));
    }

    Ok(None)
}

async fn lookup_actor_id(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "select id from access.actors where actor_type = 'user' and user_id = $1 limit 1",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn record_audit(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Option<&str>,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    scope_type: &str,
    scope_key: Option<&str>,
    before_state: Option<serde_json::Value>,
    after_state: Option<serde_json::Value>,
    ip_address: Option<String>,
) -> Result<(), ApiError> {
    audit_repo::record_audit(
        &mut **tx,
        audit_repo::AuditEntryInput {
            actor_id: actor_id.map(ToOwned::to_owned),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.map(ToOwned::to_owned),
            scope_type: scope_type.to_string(),
            scope_key: scope_key.map(ToOwned::to_owned),
            before_state,
            after_state,
            ip_address,
        },
    )
    .await
}

fn error_result(errors: Vec<ApiMessage>) -> CreateOrUpdateTrainingSessionResult {
    CreateOrUpdateTrainingSessionResult {
        session: None,
        release: None,
        roster_updates: Vec::new(),
        ots_recommendation: None,
        errors,
    }
}

fn message(message: &str) -> ApiMessage {
    ApiMessage {
        message: message.to_string(),
    }
}

fn format_zulu(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%MZ").to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterMode {
    Exact,
    Pattern,
}

fn normalize_filter_mode(operator: &str) -> FilterMode {
    match operator {
        "equals" | "=" => FilterMode::Exact,
        _ => FilterMode::Pattern,
    }
}

fn build_filter_pattern(operator: &str, value: &str) -> String {
    match operator {
        "startsWith" | "starts_with" => format!("{value}%"),
        "endsWith" | "ends_with" => format!("%{value}"),
        "equals" | "=" => value.to_string(),
        _ => format!("%{value}%"),
    }
}

trait IntoUpdateTrainingSessionRequest {
    fn into_update_request(self) -> UpdateTrainingSessionRequest;
}

impl IntoUpdateTrainingSessionRequest for CreateTrainingSessionRequest {
    fn into_update_request(self) -> UpdateTrainingSessionRequest {
        UpdateTrainingSessionRequest {
            student_id: self.student_id,
            start: self.start,
            end: self.end,
            additional_comments: self.additional_comments,
            trainer_comments: self.trainer_comments,
            enable_markdown: self.enable_markdown,
            tickets: self.tickets,
            performance_indicator: self.performance_indicator,
        }
    }
}

trait TrainingSessionRequestExt {
    fn ticket_lesson_ids(&self) -> Vec<String>;
}

impl TrainingSessionRequestExt for UpdateTrainingSessionRequest {
    fn ticket_lesson_ids(&self) -> Vec<String> {
        self.tickets
            .iter()
            .map(|ticket| ticket.lesson_id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{FilterMode, build_filter_pattern, normalize_filter_mode};

    #[test]
    fn filter_mode_treats_equals_as_exact() {
        assert_eq!(normalize_filter_mode("equals"), FilterMode::Exact);
        assert_eq!(normalize_filter_mode("="), FilterMode::Exact);
    }

    #[test]
    fn filter_pattern_builds_contains_default() {
        assert_eq!(build_filter_pattern("contains", "ZDC"), "%ZDC%");
        assert_eq!(build_filter_pattern("startsWith", "OBS"), "OBS%");
        assert_eq!(build_filter_pattern("ends_with", "CTR"), "%CTR");
        assert_eq!(build_filter_pattern("equals", "DCA_TWR"), "DCA_TWR");
    }
}
