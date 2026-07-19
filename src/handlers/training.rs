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
        context::CurrentUser,
        permissions::{
            TrainingAppointmentsCreate, TrainingAppointmentsDelete, TrainingAppointmentsRead,
            TrainingAppointmentsUpdate, TrainingAssignmentRequestsDecide,
            TrainingAssignmentRequestsInterestDelete, TrainingAssignmentRequestsInterestRequest,
            TrainingAssignmentRequestsRead, TrainingAssignmentRequestsSelfRequest,
            TrainingAssignmentsCreate, TrainingAssignmentsRead, TrainingLessonsCreate,
            TrainingLessonsDelete, TrainingLessonsRead, TrainingLessonsUpdate,
            TrainingOtsRecommendationsCreate, TrainingOtsRecommendationsDelete,
            TrainingOtsRecommendationsRead, TrainingOtsRecommendationsUpdate,
            TrainingReleaseRequestsDecide, TrainingReleaseRequestsRead,
            TrainingReleaseRequestsSelfRequest, TrainingSessionsCreate, TrainingSessionsDelete,
            TrainingSessionsRead, TrainingSessionsUpdate,
        },
        require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{
        AdditionalTrainerRequest, ApiMessage, CreateLessonRubricCellRequest,
        CreateLessonRubricCriteriaRequest, CreateOrUpdateTrainingSessionResult,
        CreateOtsRecommendationRequest, CreateTrainerReleaseRequestRequest,
        CreateTrainingAppointmentRequest, CreateTrainingAssignmentRequest,
        CreateTrainingAssignmentRequestRequest, CreateTrainingLessonRequest,
        CreateTrainingSessionRequest, DecideTrainerReleaseRequestRequest,
        DecideTrainingAssignmentRequestRequest, LessonRosterChangeSummary,
        LessonRubricCriteriaDetail, LessonRubricDetail, ListTrainingAppointmentsQuery,
        ListTrainingSessionsQuery, OtsRecommendationListResponse, OtsRecommendationSummary,
        PaginationMeta, PaginationQuery, TrainerReleaseRequest, TrainerReleaseRequestListResponse,
        TrainingAppointmentDetail, TrainingAppointmentListResponse, TrainingAssignment,
        TrainingAssignmentListResponse, TrainingAssignmentRequest,
        TrainingAssignmentRequestListResponse, TrainingLesson, TrainingLessonListResponse,
        TrainingSessionDetail, TrainingSessionListResponse, UpdateLessonRubricCellRequest,
        UpdateLessonRubricCriteriaRequest, UpdateOtsRecommendationRequest,
        UpdateTrainingAppointmentRequest, UpdateTrainingLessonRequest,
        UpdateTrainingSessionRequest,
    },
    repos::{
        audit as audit_repo,
        training::{
            appointments as training_appointments_repo,
            assignment_requests as training_assignment_requests_repo,
            assignments as training_assignments_repo, lessons as training_lessons_repo,
            ots as training_ots_repo, release_requests as training_release_requests_repo,
            rubrics as training_rubrics_repo, sessions as training_sessions_repo,
            sessions::{LessonRow, RubricMembershipRow},
        },
    },
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

const VALID_PI_MARKERS: &[&str] = &[
    "OBSERVED",
    "NOT_OBSERVED",
    "SATISFACTORY",
    "NEEDS_IMPROVEMENT",
    "UNSATISFACTORY",
];

#[derive(Debug, Clone)]
struct RubricRule {
    criteria_ids: HashSet<String>,
    cells_by_criteria: HashMap<String, HashSet<String>>,
}

#[utoipa::path(
    get,
    path = "/api/v1/training/assignments",
    tag = "training",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List assignments", body = TrainingAssignmentListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_assignments(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingAssignmentsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingAssignmentListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = training_assignments_repo::count_assignments(db).await?;
    let rows =
        training_assignments_repo::list_assignments(db, pagination.page_size, pagination.offset)
            .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        TrainingAssignmentListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
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
    _permission: RequirePermission<TrainingAssignmentsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateTrainingAssignmentRequest>,
) -> Result<(StatusCode, ApiJson<TrainingAssignment>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;

    let row = training_assignments_repo::insert_assignment(
        &mut *tx,
        &id,
        &payload.student_id,
        &payload.primary_trainer_id,
        now,
    )
    .await?;

    if let Some(other_trainer_ids) = payload.other_trainer_ids {
        for trainer_id in other_trainer_ids {
            if trainer_id == payload.primary_trainer_id {
                continue;
            }

            training_assignments_repo::insert_other_trainer(&mut *tx, &id, &trainer_id).await?;
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

    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
}

#[utoipa::path(
    get,
    path = "/api/v1/training/ots-recommendations",
    tag = "training",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List OTS recommendations", body = OtsRecommendationListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_ots_recommendations(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingOtsRecommendationsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<OtsRecommendationListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = training_ots_repo::count_ots_recommendations(db).await?;
    let rows =
        training_ots_repo::list_ots_recommendations(db, pagination.page_size, pagination.offset)
            .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        OtsRecommendationListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/ots-recommendations",
    tag = "training",
    request_body = CreateOtsRecommendationRequest,
    responses(
        (status = 201, description = "OTS recommendation created", body = OtsRecommendationSummary),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_ots_recommendation(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingOtsRecommendationsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateOtsRecommendationRequest>,
) -> Result<(StatusCode, ApiJson<OtsRecommendationSummary>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let notes = payload.notes.trim();
    if notes.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    if !training_ots_repo::user_exists(&mut *tx, &payload.student_id).await? {
        return Err(ApiError::BadRequest);
    }

    if training_ots_repo::student_has_ots_recommendation(&mut *tx, &payload.student_id).await? {
        return Err(ApiError::BadRequest);
    }

    let row = training_ots_repo::insert_ots_recommendation(
        &mut *tx,
        &Uuid::new_v4().to_string(),
        &payload.student_id,
        notes,
        Utc::now(),
    )
    .await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "CREATE",
        "OTS_RECOMMENDATION",
        Some(&row.id),
        "training_session",
        Some(row.student_id.as_str()),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/ots-recommendations/{recommendation_id}",
    tag = "training",
    params(
        ("recommendation_id" = String, Path, description = "OTS recommendation ID")
    ),
    request_body = UpdateOtsRecommendationRequest,
    responses(
        (status = 200, description = "OTS recommendation updated", body = OtsRecommendationSummary),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "OTS recommendation not found")
    )
)]
pub async fn update_ots_recommendation(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingOtsRecommendationsUpdate>,
    Path(recommendation_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateOtsRecommendationRequest>,
) -> Result<ApiJson<OtsRecommendationSummary>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let before = training_ots_repo::fetch_ots_recommendation(&mut *tx, &recommendation_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if let Some(instructor_id) = payload.assigned_instructor_id.as_deref() {
        if !training_ots_repo::user_exists(&mut *tx, instructor_id).await? {
            return Err(ApiError::BadRequest);
        }
    }

    let row = training_ots_repo::update_ots_recommendation_row(
        &mut *tx,
        &recommendation_id,
        payload.assigned_instructor_id.as_deref(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "UPDATE",
        "OTS_RECOMMENDATION",
        Some(&row.id),
        "training_session",
        Some(row.student_id.as_str()),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(ApiJson::new(row, time))
}

#[utoipa::path(
    delete,
    path = "/api/v1/training/ots-recommendations/{recommendation_id}",
    tag = "training",
    params(
        ("recommendation_id" = String, Path, description = "OTS recommendation ID")
    ),
    responses(
        (status = 204, description = "OTS recommendation deleted"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "OTS recommendation not found")
    )
)]
pub async fn delete_ots_recommendation(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingOtsRecommendationsDelete>,
    Path(recommendation_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let deleted = training_ots_repo::delete_ots_recommendation_row(&mut *tx, &recommendation_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "DELETE",
        "OTS_RECOMMENDATION",
        Some(&deleted.id),
        "training_session",
        Some(deleted.student_id.as_str()),
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
    path = "/api/v1/training/lessons",
    tag = "training",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List training lessons", body = TrainingLessonListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_lessons(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingLessonListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = training_lessons_repo::count_lessons(db).await?;
    let rows =
        training_lessons_repo::list_lessons(db, pagination.page_size, pagination.offset).await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        TrainingLessonListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
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
    _permission: RequirePermission<TrainingLessonsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateTrainingLessonRequest>,
) -> Result<(StatusCode, ApiJson<TrainingLesson>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    validate_training_lesson_payload(&payload.identifier, payload.location, payload.duration)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;
    let now = Utc::now();
    let lesson_id = Uuid::new_v4().to_string();

    let row = training_lessons_repo::insert_lesson(
        &mut *tx,
        &lesson_id,
        payload.identifier.trim(),
        payload.location,
        payload.name.trim(),
        payload.description.trim(),
        payload.position.trim(),
        payload.facility.trim(),
        now,
        payload.instructor_only,
        payload.notify_instructor_on_pass,
        payload.release_request_on_pass,
        payload.duration,
        payload.trainee_preparation.as_deref(),
        payload.performance_indicator_template_id.as_deref(),
    )
    .await?;

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

    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Lesson not found")
    )
)]
pub async fn update_lesson(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateTrainingLessonRequest>,
) -> Result<ApiJson<TrainingLesson>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    validate_training_lesson_payload(&payload.identifier, payload.location, payload.duration)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;
    let now = Utc::now();
    let before = training_lessons_repo::fetch_lesson(&mut *tx, &lesson_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let row = training_lessons_repo::update_lesson_row(
        &mut *tx,
        &lesson_id,
        payload.identifier.trim(),
        payload.location,
        payload.name.trim(),
        payload.description.trim(),
        payload.position.trim(),
        payload.facility.trim(),
        now,
        payload.instructor_only,
        payload.notify_instructor_on_pass,
        payload.release_request_on_pass,
        payload.duration,
        payload.trainee_preparation.as_deref(),
        payload.performance_indicator_template_id.as_deref(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;

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

    Ok(ApiJson::new(row, time))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Lesson not found")
    )
)]
pub async fn delete_lesson(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsDelete>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let deleted = training_lessons_repo::delete_lesson_row(&mut *tx, &lesson_id)
        .await?
        .ok_or(ApiError::NotFound)?;

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
    path = "/api/v1/training/lessons/{lesson_id}/rubric",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID")
    ),
    responses(
        (status = 200, description = "Lesson rubric", body = LessonRubricDetail),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Lesson not found or has no rubric")
    )
)]
pub async fn get_lesson_rubric(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Path(lesson_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<LessonRubricDetail>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let detail = training_rubrics_repo::fetch_lesson_rubric_detail(db, &lesson_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(ApiJson::new(detail, time))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/lessons/{lesson_id}/rubric-criteria",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID")
    ),
    request_body = CreateLessonRubricCriteriaRequest,
    responses(
        (status = 201, description = "Rubric criteria created", body = LessonRubricCriteriaDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Lesson not found")
    )
)]
pub async fn create_lesson_rubric_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(lesson_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateLessonRubricCriteriaRequest>,
) -> Result<(StatusCode, ApiJson<LessonRubricCriteriaDetail>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    validate_rubric_criteria_payload(
        &payload.criteria,
        &payload.description,
        payload.max_points,
        payload.passing,
    )?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let existing_rubric_id = training_rubrics_repo::fetch_lesson_rubric_id(&mut *tx, &lesson_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let now = Utc::now();
    let rubric_id = match existing_rubric_id {
        Some(id) => id,
        None => {
            let new_rubric_id = Uuid::new_v4().to_string();
            training_rubrics_repo::insert_rubric(&mut *tx, &new_rubric_id, now).await?;
            training_rubrics_repo::set_lesson_rubric_id(&mut *tx, &lesson_id, &new_rubric_id)
                .await?;
            new_rubric_id
        }
    };

    let criteria_id = Uuid::new_v4().to_string();
    let criteria = payload.criteria.trim().to_string();
    let description = payload.description.trim().to_string();

    training_rubrics_repo::insert_criteria(
        &mut *tx,
        &criteria_id,
        &rubric_id,
        &criteria,
        &description,
        payload.passing,
        payload.max_points,
        now,
    )
    .await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "CREATE",
        "LESSON_RUBRIC_CRITERIA",
        Some(&criteria_id),
        "training_session",
        Some(&lesson_id),
        None,
        Some(serde_json::json!({
            "id": criteria_id,
            "rubric_id": rubric_id,
            "lesson_id": lesson_id,
            "criteria": criteria,
            "description": description,
            "passing": payload.passing,
            "max_points": payload.max_points,
        })),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok((
        StatusCode::CREATED,
        ApiJson::new(
            LessonRubricCriteriaDetail {
                id: criteria_id,
                rubric_id,
                criteria,
                description,
                passing: payload.passing,
                max_points: payload.max_points,
                cells: Vec::new(),
            },
            time,
        ),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID"),
        ("criteria_id" = String, Path, description = "Rubric criteria ID")
    ),
    request_body = UpdateLessonRubricCriteriaRequest,
    responses(
        (status = 200, description = "Rubric criteria updated", body = LessonRubricCriteriaDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Rubric criteria not found")
    )
)]
pub async fn update_lesson_rubric_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path((lesson_id, criteria_id)): Path<(String, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateLessonRubricCriteriaRequest>,
) -> Result<ApiJson<LessonRubricCriteriaDetail>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    validate_rubric_criteria_payload(
        &payload.criteria,
        &payload.description,
        payload.max_points,
        payload.passing,
    )?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let before =
        training_rubrics_repo::fetch_criteria_for_lesson(&mut *tx, &lesson_id, &criteria_id)
            .await?
            .ok_or(ApiError::NotFound)?;

    let criteria = payload.criteria.trim().to_string();
    let description = payload.description.trim().to_string();

    training_rubrics_repo::update_criteria_row(
        &mut *tx,
        &criteria_id,
        &criteria,
        &description,
        payload.passing,
        payload.max_points,
        Utc::now(),
    )
    .await?;

    let cells = training_rubrics_repo::fetch_criteria_cells(&mut *tx, &criteria_id).await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "UPDATE",
        "LESSON_RUBRIC_CRITERIA",
        Some(&criteria_id),
        "training_session",
        Some(&lesson_id),
        Some(serde_json::json!({
            "id": before.id,
            "rubric_id": before.rubric_id,
            "criteria": before.criteria,
            "description": before.description,
            "passing": before.passing,
            "max_points": before.max_points,
        })),
        Some(serde_json::json!({
            "id": criteria_id,
            "rubric_id": before.rubric_id,
            "criteria": criteria,
            "description": description,
            "passing": payload.passing,
            "max_points": payload.max_points,
        })),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(ApiJson::new(
        LessonRubricCriteriaDetail {
            id: criteria_id,
            rubric_id: before.rubric_id,
            criteria,
            description,
            passing: payload.passing,
            max_points: payload.max_points,
            cells,
        },
        time,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID"),
        ("criteria_id" = String, Path, description = "Rubric criteria ID")
    ),
    responses(
        (status = 204, description = "Rubric criteria deleted"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Rubric criteria not found")
    )
)]
pub async fn delete_lesson_rubric_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsDelete>,
    Path((lesson_id, criteria_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let deleted =
        training_rubrics_repo::fetch_criteria_for_lesson(&mut *tx, &lesson_id, &criteria_id)
            .await?
            .ok_or(ApiError::NotFound)?;

    training_rubrics_repo::delete_criteria_row(&mut *tx, &criteria_id).await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "DELETE",
        "LESSON_RUBRIC_CRITERIA",
        Some(&criteria_id),
        "training_session",
        Some(&lesson_id),
        Some(serde_json::json!({
            "id": deleted.id,
            "rubric_id": deleted.rubric_id,
            "criteria": deleted.criteria,
            "description": deleted.description,
            "passing": deleted.passing,
            "max_points": deleted.max_points,
        })),
        None,
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID"),
        ("criteria_id" = String, Path, description = "Rubric criteria ID")
    ),
    request_body = CreateLessonRubricCellRequest,
    responses(
        (status = 201, description = "Rubric cell created", body = crate::models::LessonRubricCellDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Rubric criteria not found")
    )
)]
pub async fn create_lesson_rubric_cell(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path((lesson_id, criteria_id)): Path<(String, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateLessonRubricCellRequest>,
) -> Result<(StatusCode, ApiJson<crate::models::LessonRubricCellDetail>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let criteria =
        training_rubrics_repo::fetch_criteria_for_lesson(&mut *tx, &lesson_id, &criteria_id)
            .await?
            .ok_or(ApiError::NotFound)?;

    validate_rubric_cell_payload(&payload.description, payload.points, criteria.max_points)?;

    let duplicate = training_rubrics_repo::count_cells_with_points(
        &mut *tx,
        &criteria_id,
        payload.points,
        None,
    )
    .await?;
    if duplicate > 0 {
        return Err(ApiError::BadRequest);
    }

    let cell_id = Uuid::new_v4().to_string();
    let description = payload.description.trim().to_string();

    training_rubrics_repo::insert_cell(
        &mut *tx,
        &cell_id,
        &criteria_id,
        payload.points,
        &description,
        Utc::now(),
    )
    .await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "CREATE",
        "LESSON_RUBRIC_CELL",
        Some(&cell_id),
        "training_session",
        Some(&lesson_id),
        None,
        Some(serde_json::json!({
            "id": cell_id,
            "criteria_id": criteria_id,
            "points": payload.points,
            "description": description,
        })),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok((
        StatusCode::CREATED,
        ApiJson::new(
            crate::models::LessonRubricCellDetail {
                id: cell_id,
                criteria_id,
                points: payload.points,
                description,
            },
            time,
        ),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells/{cell_id}",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID"),
        ("criteria_id" = String, Path, description = "Rubric criteria ID"),
        ("cell_id" = String, Path, description = "Rubric cell ID")
    ),
    request_body = UpdateLessonRubricCellRequest,
    responses(
        (status = 200, description = "Rubric cell updated", body = crate::models::LessonRubricCellDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Rubric cell not found")
    )
)]
pub async fn update_lesson_rubric_cell(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path((lesson_id, criteria_id, cell_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateLessonRubricCellRequest>,
) -> Result<ApiJson<crate::models::LessonRubricCellDetail>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let criteria =
        training_rubrics_repo::fetch_criteria_for_lesson(&mut *tx, &lesson_id, &criteria_id)
            .await?
            .ok_or(ApiError::NotFound)?;

    let before = training_rubrics_repo::fetch_cell_for_criteria(&mut *tx, &criteria_id, &cell_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    validate_rubric_cell_payload(&payload.description, payload.points, criteria.max_points)?;

    let duplicate = training_rubrics_repo::count_cells_with_points(
        &mut *tx,
        &criteria_id,
        payload.points,
        Some(&cell_id),
    )
    .await?;
    if duplicate > 0 {
        return Err(ApiError::BadRequest);
    }

    let description = payload.description.trim().to_string();

    training_rubrics_repo::update_cell_row(&mut *tx, &cell_id, payload.points, &description)
        .await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "UPDATE",
        "LESSON_RUBRIC_CELL",
        Some(&cell_id),
        "training_session",
        Some(&lesson_id),
        Some(serde_json::json!({
            "id": before.id,
            "criteria_id": before.criteria_id,
            "points": before.points,
            "description": before.description,
        })),
        Some(serde_json::json!({
            "id": cell_id,
            "criteria_id": criteria_id,
            "points": payload.points,
            "description": description,
        })),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(ApiJson::new(
        crate::models::LessonRubricCellDetail {
            id: cell_id,
            criteria_id,
            points: payload.points,
            description,
        },
        time,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells/{cell_id}",
    tag = "training",
    params(
        ("lesson_id" = String, Path, description = "Training lesson ID"),
        ("criteria_id" = String, Path, description = "Rubric criteria ID"),
        ("cell_id" = String, Path, description = "Rubric cell ID")
    ),
    responses(
        (status = 204, description = "Rubric cell deleted"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Rubric cell not found")
    )
)]
pub async fn delete_lesson_rubric_cell(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsDelete>,
    Path((lesson_id, criteria_id, cell_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    training_rubrics_repo::fetch_criteria_for_lesson(&mut *tx, &lesson_id, &criteria_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let deleted = training_rubrics_repo::fetch_cell_for_criteria(&mut *tx, &criteria_id, &cell_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    training_rubrics_repo::delete_cell_row(&mut *tx, &cell_id).await?;

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "DELETE",
        "LESSON_RUBRIC_CELL",
        Some(&cell_id),
        "training_session",
        Some(&lesson_id),
        Some(serde_json::json!({
            "id": deleted.id,
            "criteria_id": deleted.criteria_id,
            "points": deleted.points,
            "description": deleted.description,
        })),
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
    params(PaginationQuery),
    responses(
        (status = 200, description = "List assignment requests", body = TrainingAssignmentRequestListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_assignment_requests(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingAssignmentRequestsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingAssignmentRequestListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = training_assignment_requests_repo::count_assignment_requests(db).await?;
    let rows = training_assignment_requests_repo::list_assignment_requests(
        db,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        TrainingAssignmentRequestListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
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
    _permission: RequirePermission<TrainingAssignmentRequestsSelfRequest>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(_payload): Json<CreateTrainingAssignmentRequestRequest>,
) -> Result<(StatusCode, ApiJson<TrainingAssignmentRequest>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let row = training_assignment_requests_repo::insert_assignment_request(db, &id, &user.id, now)
        .await?;

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

    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Assignment request not found")
    )
)]
pub async fn decide_assignment_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingAssignmentRequestsDecide>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<DecideTrainingAssignmentRequestRequest>,
) -> Result<ApiJson<TrainingAssignmentRequest>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "APPROVED" && normalized_status != "DENIED" {
        return Err(ApiError::BadRequest);
    }

    let now = Utc::now();
    let before = training_assignment_requests_repo::fetch_assignment_request(db, &request_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let row = training_assignment_requests_repo::decide_assignment_request_row(
        db,
        &request_id,
        &normalized_status,
        now,
        &user.id,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

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

    Ok(ApiJson::new(row, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/training/trainer-release-requests",
    tag = "training",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List trainer release requests", body = TrainerReleaseRequestListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_release_requests(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingReleaseRequestsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainerReleaseRequestListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = training_release_requests_repo::count_release_requests(db).await?;
    let rows = training_release_requests_repo::list_release_requests(
        db,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        TrainerReleaseRequestListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
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
    _permission: RequirePermission<TrainingReleaseRequestsSelfRequest>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(_payload): Json<CreateTrainerReleaseRequestRequest>,
) -> Result<(StatusCode, ApiJson<TrainerReleaseRequest>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let row =
        training_release_requests_repo::insert_release_request(db, &id, &user.id, now).await?;

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

    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Release request not found")
    )
)]
pub async fn decide_release_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingReleaseRequestsDecide>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<DecideTrainerReleaseRequestRequest>,
) -> Result<ApiJson<TrainerReleaseRequest>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let normalized_status = payload.status.trim().to_ascii_uppercase();
    if normalized_status != "APPROVED" && normalized_status != "DENIED" {
        return Err(ApiError::BadRequest);
    }

    let now = Utc::now();
    let before = training_release_requests_repo::fetch_release_request(db, &request_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let row = training_release_requests_repo::decide_release_request_row(
        db,
        &request_id,
        &normalized_status,
        now,
        &user.id,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

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

    Ok(ApiJson::new(row, time))
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
    _permission: RequirePermission<TrainingAssignmentRequestsInterestRequest>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    if !training_assignment_requests_repo::assignment_request_exists(db, &request_id).await? {
        return Err(ApiError::BadRequest);
    }

    training_assignment_requests_repo::add_interested_trainer(db, &request_id, &user.id).await?;

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
    _permission: RequirePermission<TrainingAssignmentRequestsInterestDelete>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    training_assignment_requests_repo::remove_interested_trainer(db, &request_id, &user.id).await?;

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
    path = "/api/v1/training/appointments",
    tag = "training",
    params(
        PaginationQuery,
        ("sort_field" = Option<String>, Query, description = "Sort field"),
        ("sort_order" = Option<String>, Query, description = "Sort order"),
        ("trainer_id" = Option<String>, Query, description = "Optional trainer filter"),
        ("student_id" = Option<String>, Query, description = "Optional student filter"),
        ("user_id" = Option<String>, Query, description = "Optional shared user filter")
    ),
    responses(
        (status = 200, description = "List training appointments", body = TrainingAppointmentListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_training_appointments(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingAppointmentsRead>,
    Query(query): Query<ListTrainingAppointmentsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingAppointmentListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let sort_column = match query.sort_field.as_deref() {
        Some("created_at") => "ta.created_at",
        Some("updated_at") => "ta.updated_at",
        _ => "ta.start",
    };
    let sort_direction = match query.sort_order.as_deref() {
        Some(value) if value.eq_ignore_ascii_case("desc") => "desc",
        _ => "asc",
    };

    let total = training_appointments_repo::count_appointments(
        db,
        query.trainer_id.as_deref(),
        query.student_id.as_deref(),
        query.user_id.as_deref(),
    )
    .await?;

    let items = training_appointments_repo::list_appointments(
        db,
        query.trainer_id.as_deref(),
        query.student_id.as_deref(),
        query.user_id.as_deref(),
        sort_column,
        sort_direction,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        TrainingAppointmentListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/training/appointments/{appointment_id}",
    tag = "training",
    params(
        ("appointment_id" = String, Path, description = "Training appointment ID")
    ),
    responses(
        (status = 200, description = "Training appointment detail", body = TrainingAppointmentDetail),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Training appointment not found")
    )
)]
pub async fn get_training_appointment(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingAppointmentsRead>,
    Path(appointment_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingAppointmentDetail>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let detail = training_appointments_repo::fetch_appointment_detail(db, &appointment_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(ApiJson::new(detail, time))
}

#[utoipa::path(
    post,
    path = "/api/v1/training/appointments",
    tag = "training",
    request_body = CreateTrainingAppointmentRequest,
    responses(
        (status = 201, description = "Training appointment created", body = TrainingAppointmentDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Student not found")
    )
)]
pub async fn create_training_appointment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingAppointmentsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateTrainingAppointmentRequest>,
) -> Result<(StatusCode, ApiJson<TrainingAppointmentDetail>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;
    let lesson_ids = validate_appointment_lesson_ids(&payload.lesson_ids)?;
    let student_id = validate_user_exists(&mut tx, &payload.student_id).await?;
    training_appointments_repo::resolve_appointment_lessons(&mut *tx, &lesson_ids).await?;
    let notes = validate_appointment_notes(payload.notes.as_deref())?;
    let additional_trainers =
        validate_appointment_additional_trainers(&payload.additional_trainers, &user.id)?;
    ensure_additional_trainers_exist(&mut tx, &additional_trainers).await?;

    let appointment_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let environment = normalize_optional_text(payload.environment.as_deref());

    training_appointments_repo::insert_appointment(
        &mut *tx,
        &appointment_id,
        &student_id,
        &user.id,
        payload.start,
        environment.as_deref(),
        &notes,
        now,
    )
    .await?;

    training_appointments_repo::replace_appointment_lessons(&mut tx, &appointment_id, &lesson_ids)
        .await?;

    for trainer in &additional_trainers {
        training_appointments_repo::insert_appointment_additional_trainer_row(
            &mut tx,
            &appointment_id,
            &trainer.trainer_id,
            &trainer.description,
        )
        .await?;
    }

    let created_snapshot = serde_json::json!({
        "id": appointment_id,
        "student_id": student_id,
        "trainer_id": user.id,
        "start": payload.start,
        "environment": environment,
        "lesson_ids": lesson_ids,
        "notes": notes,
        "additional_trainers": additional_trainers,
        "double_booking": false,
        "preparation_completed": false,
        "warning_email_sent": false,
        "atc_booking_id": null
    });
    let created_scope_key = created_snapshot["student_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let created_resource_id = created_snapshot["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "CREATE",
        "TRAINING_APPOINTMENT",
        Some(&created_resource_id),
        "training_session",
        Some(&created_scope_key),
        None,
        Some(created_snapshot),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let detail = training_appointments_repo::fetch_appointment_detail(db, &appointment_id)
        .await?
        .ok_or(ApiError::Internal)?;

    Ok((StatusCode::CREATED, ApiJson::new(detail, time)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/training/appointments/{appointment_id}",
    tag = "training",
    params(
        ("appointment_id" = String, Path, description = "Training appointment ID")
    ),
    request_body = UpdateTrainingAppointmentRequest,
    responses(
        (status = 200, description = "Training appointment updated", body = TrainingAppointmentDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Training appointment not found")
    )
)]
pub async fn update_training_appointment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingAppointmentsUpdate>,
    Path(appointment_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateTrainingAppointmentRequest>,
) -> Result<ApiJson<TrainingAppointmentDetail>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let existing = training_appointments_repo::fetch_appointment_row(&mut *tx, &appointment_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let existing_lesson_ids =
        training_appointments_repo::fetch_appointment_lesson_ids(&mut *tx, &appointment_id).await?;

    let existing_additional_trainers =
        training_appointments_repo::fetch_appointment_additional_trainers(
            &mut *tx,
            &appointment_id,
        )
        .await?;

    let lesson_ids = validate_appointment_lesson_ids(&payload.lesson_ids)?;
    let student_id = validate_user_exists(&mut tx, &payload.student_id).await?;
    training_appointments_repo::resolve_appointment_lessons(&mut *tx, &lesson_ids).await?;
    let notes = validate_appointment_notes(payload.notes.as_deref())?;
    let additional_trainers =
        validate_appointment_additional_trainers(&payload.additional_trainers, &user.id)?;
    ensure_additional_trainers_exist(&mut tx, &additional_trainers).await?;

    let environment = normalize_optional_text(payload.environment.as_deref());
    let atc_booking_id = payload
        .atc_booking_id
        .as_ref()
        .map(|value| normalize_optional_text(value.as_deref()));

    training_appointments_repo::update_appointment_row(
        &mut *tx,
        &appointment_id,
        &student_id,
        payload.start,
        environment.as_deref(),
        payload.double_booking.unwrap_or(existing.double_booking),
        payload
            .preparation_completed
            .unwrap_or(existing.preparation_completed),
        payload
            .warning_email_sent
            .unwrap_or(existing.warning_email_sent),
        atc_booking_id
            .as_ref()
            .map(|value| value.as_deref())
            .unwrap_or(existing.atc_booking_id.as_deref()),
        &notes,
        Utc::now(),
    )
    .await?;

    training_appointments_repo::replace_appointment_lessons(&mut tx, &appointment_id, &lesson_ids)
        .await?;

    training_appointments_repo::delete_appointment_additional_trainers(&mut tx, &appointment_id)
        .await?;
    for trainer in &additional_trainers {
        training_appointments_repo::insert_appointment_additional_trainer_row(
            &mut tx,
            &appointment_id,
            &trainer.trainer_id,
            &trainer.description,
        )
        .await?;
    }

    let before_snapshot = serde_json::json!({
        "id": existing.id,
        "student_id": existing.student_id,
        "trainer_id": existing.trainer_id,
        "start": existing.start,
        "environment": existing.environment,
        "double_booking": existing.double_booking,
        "preparation_completed": existing.preparation_completed,
        "warning_email_sent": existing.warning_email_sent,
        "atc_booking_id": existing.atc_booking_id,
        "notes": existing.notes,
        "additional_trainers": existing_additional_trainers,
        "lesson_ids": existing_lesson_ids
    });
    let after_snapshot = serde_json::json!({
        "id": appointment_id,
        "student_id": student_id,
        "trainer_id": existing.trainer_id,
        "start": payload.start,
        "environment": environment,
        "double_booking": payload.double_booking.unwrap_or(existing.double_booking),
        "preparation_completed": payload.preparation_completed.unwrap_or(existing.preparation_completed),
        "warning_email_sent": payload.warning_email_sent.unwrap_or(existing.warning_email_sent),
        "atc_booking_id": atc_booking_id.unwrap_or(existing.atc_booking_id),
        "notes": notes,
        "additional_trainers": additional_trainers,
        "lesson_ids": lesson_ids
    });
    let update_scope_key = after_snapshot["student_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "UPDATE",
        "TRAINING_APPOINTMENT",
        Some(&appointment_id),
        "training_session",
        Some(&update_scope_key),
        Some(before_snapshot),
        Some(after_snapshot),
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let detail = training_appointments_repo::fetch_appointment_detail(db, &appointment_id)
        .await?
        .ok_or(ApiError::Internal)?;

    Ok(ApiJson::new(detail, time))
}

#[utoipa::path(
    delete,
    path = "/api/v1/training/appointments/{appointment_id}",
    tag = "training",
    params(
        ("appointment_id" = String, Path, description = "Training appointment ID")
    ),
    responses(
        (status = 204, description = "Training appointment deleted"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Training appointment not found")
    )
)]
pub async fn delete_training_appointment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingAppointmentsDelete>,
    Path(appointment_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let lesson_ids =
        training_appointments_repo::fetch_appointment_lesson_ids(&mut *tx, &appointment_id).await?;

    let deleted = training_appointments_repo::delete_appointment_row(&mut *tx, &appointment_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let deleted_snapshot = serde_json::json!({
        "id": deleted.id,
        "student_id": deleted.student_id,
        "trainer_id": deleted.trainer_id,
        "start": deleted.start,
        "environment": deleted.environment,
        "double_booking": deleted.double_booking,
        "preparation_completed": deleted.preparation_completed,
        "warning_email_sent": deleted.warning_email_sent,
        "atc_booking_id": deleted.atc_booking_id,
        "lesson_ids": lesson_ids
    });
    let delete_scope_key = deleted_snapshot["student_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    record_audit(
        &mut tx,
        actor_id.as_deref(),
        "DELETE",
        "TRAINING_APPOINTMENT",
        Some(&appointment_id),
        "training_session",
        Some(&delete_scope_key),
        Some(deleted_snapshot),
        None,
        audit_repo::client_ip(&headers),
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/training/sessions",
    tag = "training",
    params(
        PaginationQuery,
        ("sort_field" = Option<String>, Query, description = "Sort field"),
        ("sort_order" = Option<String>, Query, description = "Sort order"),
        ("filter_field" = Option<String>, Query, description = "Filter field"),
        ("filter_operator" = Option<String>, Query, description = "Filter operator"),
        ("filter_value" = Option<String>, Query, description = "Filter value"),
        ("student_id" = Option<String>, Query, description = "Optional student filter"),
        ("instructor_id" = Option<String>, Query, description = "Optional instructor filter")
    ),
    responses(
        (status = 200, description = "List training sessions", body = TrainingSessionListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_training_sessions(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingSessionsRead>,
    Query(query): Query<ListTrainingSessionsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingSessionListResponse>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
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

    let count = training_sessions_repo::count_sessions(
        db,
        query.student_id.as_deref(),
        query.instructor_id.as_deref(),
        &filter_field,
        &filter_pattern,
        filter_is_exact,
    )
    .await?;

    let mut items = training_sessions_repo::list_sessions(
        db,
        query.student_id.as_deref(),
        query.instructor_id.as_deref(),
        &filter_field,
        &filter_pattern,
        filter_is_exact,
        sort_column,
        sort_direction,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    if count == 0 {
        items.clear();
    }

    let meta = PaginationMeta::new(count, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        TrainingSessionListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Training session not found")
    )
)]
pub async fn get_training_session(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingSessionsRead>,
    Path(session_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingSessionDetail>, ApiError> {
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let detail = training_sessions_repo::fetch_session_detail(db, &session_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(ApiJson::new(detail, time))
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
    _permission: RequirePermission<TrainingSessionsCreate>,
    time: ResponseTimeContext,
    Json(payload): Json<CreateTrainingSessionRequest>,
) -> Result<(StatusCode, ApiJson<CreateOrUpdateTrainingSessionResult>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    match upsert_training_session(db, user, None, payload.into_update_request()).await? {
        Ok(result) => Ok((StatusCode::CREATED, ApiJson::new(result, time.clone()))),
        Err(errors) => Ok((
            StatusCode::BAD_REQUEST,
            ApiJson::new(error_result(errors), time),
        )),
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Training session not found")
    )
)]
pub async fn update_training_session(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingSessionsUpdate>,
    Path(session_id): Path<String>,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateTrainingSessionRequest>,
) -> Result<ApiJson<CreateOrUpdateTrainingSessionResult>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    match upsert_training_session(db, user, Some(session_id), payload).await? {
        Ok(result) => Ok(ApiJson::new(result, time)),
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Training session not found")
    )
)]
pub async fn delete_training_session(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingSessionsDelete>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let db = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let deleted = training_sessions_repo::delete_session_row(&mut tx, &session_id)
        .await?
        .ok_or(ApiError::NotFound)?;

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

fn validate_rubric_criteria_payload(
    criteria: &str,
    description: &str,
    max_points: i32,
    passing: i32,
) -> Result<(), ApiError> {
    if criteria.trim().is_empty()
        || criteria.trim().chars().count() > 255
        || description.trim().is_empty()
        || max_points < 1
        || passing < 0
    {
        return Err(ApiError::BadRequest);
    }

    Ok(())
}

fn validate_rubric_cell_payload(
    description: &str,
    points: i32,
    criteria_max_points: i32,
) -> Result<(), ApiError> {
    if description.trim().is_empty()
        || description.trim().chars().count() > 255
        || points < 0
        || points > criteria_max_points
    {
        return Err(ApiError::BadRequest);
    }

    Ok(())
}

fn validate_appointment_lesson_ids(lesson_ids: &[String]) -> Result<Vec<String>, ApiError> {
    if lesson_ids.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(lesson_ids.len());
    for lesson_id in lesson_ids {
        let lesson_id = lesson_id.trim();
        if lesson_id.is_empty() || !seen.insert(lesson_id.to_string()) {
            return Err(ApiError::BadRequest);
        }

        normalized.push(lesson_id.to_string());
    }

    Ok(normalized)
}

fn validate_appointment_notes(notes: Option<&str>) -> Result<String, ApiError> {
    let notes = notes.unwrap_or("").trim().to_ascii_uppercase();
    if notes.chars().count() > 50 {
        return Err(ApiError::BadRequest);
    }

    Ok(notes)
}

fn validate_appointment_additional_trainers(
    additional_trainers: &[AdditionalTrainerRequest],
    forbidden_trainer_id: &str,
) -> Result<Vec<AdditionalTrainerRequest>, ApiError> {
    let forbidden_ids: HashSet<&str> = [forbidden_trainer_id].into_iter().collect();
    if !validate_additional_trainer_shapes(additional_trainers, &forbidden_ids).is_empty() {
        return Err(ApiError::BadRequest);
    }

    Ok(additional_trainers
        .iter()
        .map(|trainer| AdditionalTrainerRequest {
            trainer_id: trainer.trainer_id.trim().to_string(),
            description: trainer.description.trim().to_ascii_uppercase(),
        })
        .collect())
}

async fn ensure_additional_trainers_exist(
    tx: &mut Transaction<'_, Postgres>,
    additional_trainers: &[AdditionalTrainerRequest],
) -> Result<(), ApiError> {
    if additional_trainers.is_empty() {
        return Ok(());
    }

    let trainer_ids = additional_trainers
        .iter()
        .map(|trainer| trainer.trainer_id.clone())
        .collect::<Vec<_>>();
    let found =
        training_appointments_repo::fetch_user_identities_by_ids(&mut **tx, &trainer_ids).await?;
    if found.len() != trainer_ids.len() {
        return Err(ApiError::BadRequest);
    }

    Ok(())
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

async fn validate_user_exists(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<String, ApiError> {
    let normalized = user_id.trim();
    if normalized.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let exists = training_appointments_repo::user_exists(&mut **tx, normalized).await?;

    exists.ok_or(ApiError::NotFound)
}

async fn upsert_training_session(
    db: &sqlx::PgPool,
    user: &CurrentUser,
    session_id: Option<String>,
    payload: UpdateTrainingSessionRequest,
) -> Result<Result<CreateOrUpdateTrainingSessionResult, Vec<ApiMessage>>, ApiError> {
    let mut tx = db.begin().await.map_err(|_| ApiError::Internal)?;
    let actor_id = lookup_actor_id(&mut tx, &user.id).await?;

    let student =
        training_sessions_repo::fetch_student_identity(&mut tx, &payload.student_id).await?;

    let Some(student) = student else {
        return Ok(Err(vec![message("Student does not exist.")]));
    };

    let lessons =
        training_sessions_repo::fetch_lessons_by_ids(&mut tx, &payload.ticket_lesson_ids()).await?;

    let lesson_map = lessons
        .into_iter()
        .map(|lesson| (lesson.id.clone(), lesson))
        .collect::<HashMap<_, _>>();

    let rubric_rows =
        training_sessions_repo::fetch_rubric_membership_rows(&mut tx, &payload.ticket_lesson_ids())
            .await?;

    let rules = build_rubric_rules(rubric_rows);
    let mut validation_errors = validate_training_session_payload(&payload, &lesson_map, &rules);

    let forbidden_trainer_ids: HashSet<&str> = [student.id.as_str(), user.id.as_str()]
        .into_iter()
        .collect();
    validation_errors.extend(validate_additional_trainer_shapes(
        &payload.additional_trainers,
        &forbidden_trainer_ids,
    ));

    if !validation_errors.is_empty() {
        return Ok(Err(validation_errors));
    }

    if !payload.additional_trainers.is_empty() {
        let trainer_ids = payload
            .additional_trainers
            .iter()
            .map(|trainer| trainer.trainer_id.clone())
            .collect::<Vec<_>>();
        let found =
            training_sessions_repo::fetch_user_identities_by_ids(&mut tx, &trainer_ids).await?;
        if found.len() != trainer_ids.len() {
            return Ok(Err(vec![message(
                "One or more additional trainers do not exist.",
            )]));
        }
    }

    let membership = training_sessions_repo::fetch_membership_row(&mut tx, &student.id).await?;

    let now = Utc::now();
    let existing_id = session_id.clone();
    let (session_id, _instructor_id, old_tickets) = if let Some(ref id) = existing_id {
        let existing = training_sessions_repo::fetch_session_exists_row(&mut tx, id)
            .await?
            .ok_or(ApiError::NotFound)?;

        let old_tickets = training_sessions_repo::fetch_old_tickets(&mut tx, id).await?;

        training_sessions_repo::delete_session_performance_indicators(&mut tx, id).await?;
        training_sessions_repo::delete_session_tickets(&mut tx, id).await?;
        training_sessions_repo::delete_session_additional_trainers(&mut tx, id).await?;

        training_sessions_repo::update_session_row(
            &mut tx,
            id,
            &payload.student_id,
            payload.start,
            payload.end,
            payload.additional_comments.as_deref(),
            payload.trainer_comments.as_deref(),
            payload.enable_markdown.unwrap_or(false),
            now,
        )
        .await?;

        (existing.id, existing.instructor_id, old_tickets)
    } else {
        let new_id = Uuid::new_v4().to_string();
        training_sessions_repo::insert_session_row(
            &mut tx,
            &new_id,
            &payload.student_id,
            &user.id,
            payload.start,
            payload.end,
            payload.additional_comments.as_deref(),
            payload.trainer_comments.as_deref(),
            payload.enable_markdown.unwrap_or(false),
            now,
        )
        .await?;

        (new_id, user.id.clone(), Vec::new())
    };

    let mut new_passed_lessons = Vec::new();
    for ticket in &payload.tickets {
        let ticket_id = Uuid::new_v4().to_string();
        training_sessions_repo::insert_ticket_row(
            &mut tx,
            &ticket_id,
            &session_id,
            &ticket.lesson_id,
            ticket.passed,
            now,
        )
        .await?;

        for score in &ticket.scores {
            training_sessions_repo::insert_rubric_score_row(
                &mut tx,
                &Uuid::new_v4().to_string(),
                &ticket_id,
                &score.criteria_id,
                &score.cell_id,
                score.passed,
            )
            .await?;
        }

        if ticket.passed {
            if let Some(lesson) = lesson_map.get(&ticket.lesson_id) {
                new_passed_lessons.push(lesson.clone());
            }
        }
    }

    for trainer in &payload.additional_trainers {
        training_sessions_repo::insert_session_additional_trainer_row(
            &mut tx,
            &session_id,
            &trainer.trainer_id,
            trainer.description.trim(),
        )
        .await?;
    }

    if let Some(ref indicator) = payload.performance_indicator {
        let indicator_id = Uuid::new_v4().to_string();
        training_sessions_repo::insert_performance_indicator_row(
            &mut tx,
            &indicator_id,
            &session_id,
            now,
        )
        .await?;

        for category in &indicator.categories {
            let category_id = Uuid::new_v4().to_string();
            training_sessions_repo::insert_performance_indicator_category_row(
                &mut tx,
                &category_id,
                &indicator_id,
                &category.name,
                category.order,
            )
            .await?;

            for criteria in &category.criteria {
                training_sessions_repo::insert_performance_indicator_criteria_row(
                    &mut tx,
                    &Uuid::new_v4().to_string(),
                    &category_id,
                    &criteria.name,
                    criteria.order,
                    &criteria.marker.trim().to_ascii_uppercase(),
                    criteria.comments.as_deref(),
                )
                .await?;
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

    let session = training_sessions_repo::fetch_session_detail(db, &session_id)
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

fn validate_additional_trainer_shapes(
    additional_trainers: &[AdditionalTrainerRequest],
    forbidden_ids: &HashSet<&str>,
) -> Vec<ApiMessage> {
    let mut errors = Vec::new();
    let mut seen = HashSet::new();

    for trainer in additional_trainers {
        let trainer_id = trainer.trainer_id.trim();
        if trainer_id.is_empty() {
            errors.push(message("You must select an additional trainer."));
            continue;
        }
        if trainer.description.trim().is_empty() {
            errors.push(message(
                "You must provide a description for the additional trainer.",
            ));
        }
        if forbidden_ids.contains(trainer_id) {
            errors.push(message(
                "You cannot add the student or yourself as an additional trainer.",
            ));
        }
        if !seen.insert(trainer_id.to_string()) {
            errors.push(message("Duplicate additional trainers are not allowed."));
        }
    }

    errors
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

    let updates = training_sessions_repo::fetch_lesson_roster_changes(tx, &lesson_ids).await?;

    let now = Utc::now();
    for update in &updates {
        training_sessions_repo::delete_solo_certification_for_roster(
            tx,
            student_user_id,
            &update.certification_type_id,
        )
        .await?;

        training_sessions_repo::upsert_user_certification(
            tx,
            &Uuid::new_v4().to_string(),
            student_user_id,
            &update.certification_type_id,
            &update.certification_option,
            now,
            actor_id,
        )
        .await?;

        training_sessions_repo::insert_dossier_entry(
            tx,
            &Uuid::new_v4().to_string(),
            student_user_id,
            writer_user_id,
            &update
                .dossier_text
                .replace("{cid}", &student_cid.to_string()),
            now,
        )
        .await?;
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

    let assignment =
        training_sessions_repo::fetch_assignment_for_student(tx, student_user_id).await?;
    if assignment.is_none() {
        return Ok(None);
    }

    let existing =
        training_sessions_repo::fetch_existing_release_request_for_student(tx, student_user_id)
            .await?;
    if existing.is_some() {
        return Ok(None);
    }

    let now = Utc::now();
    let row = training_sessions_repo::insert_release_request_from_session(
        tx,
        &Uuid::new_v4().to_string(),
        student_user_id,
        now,
    )
    .await?;

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
        let deleted_ids =
            training_sessions_repo::delete_ots_recommendations_for_student(tx, student_user_id)
                .await?;

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

        let existing =
            training_sessions_repo::fetch_ots_recommendation_for_student(tx, student_user_id)
                .await?;
        if existing.is_some() {
            return Ok(None);
        }

        let now = Utc::now();
        let note = format!(
            "{} w/ {} ON {}.",
            lesson.identifier,
            trainer_name,
            format_zulu(start)
        );

        let rec = training_sessions_repo::insert_ots_recommendation_note(
            tx,
            &Uuid::new_v4().to_string(),
            student_user_id,
            &note,
            now,
        )
        .await?;

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
            additional_trainers: self.additional_trainers,
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
