use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath, fetch_user_access},
        context::CurrentUser,
        middleware::ensure_permission,
        permissions::{TrainingDossierCreate, TrainingLessonsRead, TrainingLessonsUpdate},
        require_permission::RequirePermission,
    },
    email::service::EmailActor,
    errors::ApiError,
    models::{
        CreateDossierEntryRequest, CreatePerformanceIndicatorCategoryRequest,
        CreatePerformanceIndicatorCriteriaRequest, CreatePerformanceIndicatorTemplateRequest,
        CreateProgressionAssignmentRequest, CreateTrainingProgressionRequest,
        CreateTrainingProgressionStepRequest, DossierEntryItem, DossierEntryListResponse,
        PaginationMeta, PaginationQuery, PerformanceIndicatorCategoryItem,
        PerformanceIndicatorCategoryListResponse, PerformanceIndicatorCriteriaItem,
        PerformanceIndicatorCriteriaListResponse, PerformanceIndicatorTemplateItem,
        PerformanceIndicatorTemplateListResponse, ProgressionAssignmentItem,
        ProgressionAssignmentListResponse, ProgressionStatusResponse, ProgressionStatusStep,
        TrainingProgressionItem, TrainingProgressionListResponse, TrainingProgressionStepItem,
        TrainingProgressionStepListResponse, UpdatePerformanceIndicatorCategoryRequest,
        UpdatePerformanceIndicatorCriteriaRequest, UpdatePerformanceIndicatorTemplateRequest,
        UpdateTrainingProgressionRequest, UpdateTrainingProgressionStepRequest,
    },
    repos::{
        access as access_repo, audit as audit_repo, training_admin as training_admin_repo,
        users as user_repo,
    },
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};
use serde_json::json;

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

#[utoipa::path(get, path = "/api/v1/admin/training/progressions", tag = "training", params(PaginationQuery), responses((status = 200, description = "Training progressions", body = TrainingProgressionListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.read")))]
pub async fn list_progressions(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingProgressionListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let total = training_admin_repo::count_progressions(pool).await?;
    let rows =
        training_admin_repo::list_progressions(pool, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        TrainingProgressionListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/training/progressions", tag = "training", request_body = CreateTrainingProgressionRequest, responses((status = 201, description = "Training progression created", body = TrainingProgressionItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update")))]
pub async fn create_progression(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateTrainingProgressionRequest>,
) -> Result<(StatusCode, ApiJson<TrainingProgressionItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = training_admin_repo::insert_progression(
        pool,
        &Uuid::new_v4().to_string(),
        payload.name.trim(),
        payload.next_progression_id.as_deref(),
        payload.auto_assign_new_home_obs.unwrap_or(false),
        payload.auto_assign_new_visitor.unwrap_or(false),
    )
    .await?;
    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "TRAINING_PROGRESSION",
        Some(row.id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/progressions/{progression_id}", tag = "training", params(("progression_id" = String, Path, description = "Progression ID")), request_body = UpdateTrainingProgressionRequest, responses((status = 200, description = "Updated training progression", body = TrainingProgressionItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Progression not found")))]
pub async fn update_progression(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(progression_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateTrainingProgressionRequest>,
) -> Result<ApiJson<TrainingProgressionItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_progression(pool, &progression_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = training_admin_repo::update_progression_row(
        pool,
        &progression_id,
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload.next_progression_id.is_some(),
        payload.next_progression_id.flatten(),
        payload.auto_assign_new_home_obs,
        payload.auto_assign_new_visitor,
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "TRAINING_PROGRESSION",
        Some(progression_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok(ApiJson::new(row, time))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/progressions/{progression_id}", tag = "training", params(("progression_id" = String, Path, description = "Progression ID")), responses((status = 200, description = "Deleted training progression", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Progression not found")))]
pub async fn delete_progression(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(progression_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_progression(pool, &progression_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    training_admin_repo::delete_progression_row(pool, &progression_id).await?;
    record_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "TRAINING_PROGRESSION",
        Some(progression_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "progression deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/training/progression-steps", tag = "training", params(PaginationQuery), responses((status = 200, description = "Training progression steps", body = TrainingProgressionStepListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.read")))]
pub async fn list_progression_steps(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<TrainingProgressionStepListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let total = training_admin_repo::count_progression_steps(pool).await?;
    let rows =
        training_admin_repo::list_progression_steps(pool, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        TrainingProgressionStepListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/training/progression-steps", tag = "training", request_body = CreateTrainingProgressionStepRequest, responses((status = 201, description = "Training progression step created", body = TrainingProgressionStepItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update")))]
pub async fn create_progression_step(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateTrainingProgressionStepRequest>,
) -> Result<(StatusCode, ApiJson<TrainingProgressionStepItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = training_admin_repo::insert_progression_step(
        pool,
        &Uuid::new_v4().to_string(),
        &payload.progression_id,
        &payload.lesson_id,
        payload.sort_order,
        payload.optional.unwrap_or(false),
    )
    .await?;
    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "TRAINING_PROGRESSION_STEP",
        Some(row.id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/progression-steps/{step_id}", tag = "training", params(("step_id" = String, Path, description = "Progression step ID")), request_body = UpdateTrainingProgressionStepRequest, responses((status = 200, description = "Updated training progression step", body = TrainingProgressionStepItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Progression step not found")))]
pub async fn update_progression_step(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(step_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateTrainingProgressionStepRequest>,
) -> Result<ApiJson<TrainingProgressionStepItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_progression_step(pool, &step_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = training_admin_repo::update_progression_step_row(
        pool,
        &step_id,
        payload.lesson_id.as_deref(),
        payload.sort_order,
        payload.optional,
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "TRAINING_PROGRESSION_STEP",
        Some(step_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok(ApiJson::new(row, time))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/progression-steps/{step_id}", tag = "training", params(("step_id" = String, Path, description = "Progression step ID")), responses((status = 200, description = "Deleted training progression step", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Progression step not found")))]
pub async fn delete_progression_step(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(step_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_progression_step(pool, &step_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    training_admin_repo::delete_progression_step_row(pool, &step_id).await?;
    record_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "TRAINING_PROGRESSION_STEP",
        Some(step_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "progression step deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/training/performance-indicators/templates", tag = "training", params(PaginationQuery), responses((status = 200, description = "Performance indicator templates", body = PerformanceIndicatorTemplateListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.read")))]
pub async fn list_performance_indicator_templates(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<PerformanceIndicatorTemplateListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let total = training_admin_repo::count_pi_templates(pool).await?;
    let rows =
        training_admin_repo::list_pi_templates(pool, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        PerformanceIndicatorTemplateListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/training/performance-indicators/templates", tag = "training", request_body = CreatePerformanceIndicatorTemplateRequest, responses((status = 201, description = "Performance indicator template created", body = PerformanceIndicatorTemplateItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update")))]
pub async fn create_performance_indicator_template(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreatePerformanceIndicatorTemplateRequest>,
) -> Result<(StatusCode, ApiJson<PerformanceIndicatorTemplateItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = training_admin_repo::insert_pi_template(
        pool,
        &Uuid::new_v4().to_string(),
        payload.name.trim(),
    )
    .await?;
    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "PERFORMANCE_INDICATOR_TEMPLATE",
        Some(row.id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/performance-indicators/templates/{template_id}", tag = "training", params(("template_id" = String, Path, description = "Template ID")), request_body = UpdatePerformanceIndicatorTemplateRequest, responses((status = 200, description = "Updated performance indicator template", body = PerformanceIndicatorTemplateItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Template not found")))]
pub async fn update_performance_indicator_template(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(template_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdatePerformanceIndicatorTemplateRequest>,
) -> Result<ApiJson<PerformanceIndicatorTemplateItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_pi_template(pool, &template_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = training_admin_repo::update_pi_template_row(pool, &template_id, payload.name.trim())
        .await?
        .ok_or(ApiError::NotFound)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "PERFORMANCE_INDICATOR_TEMPLATE",
        Some(template_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok(ApiJson::new(row, time))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/performance-indicators/templates/{template_id}", tag = "training", params(("template_id" = String, Path, description = "Template ID")), responses((status = 200, description = "Deleted performance indicator template", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Template not found")))]
pub async fn delete_performance_indicator_template(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(template_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_pi_template(pool, &template_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    training_admin_repo::delete_pi_template_row(pool, &template_id).await?;
    record_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "PERFORMANCE_INDICATOR_TEMPLATE",
        Some(template_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "performance indicator template deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/training/performance-indicators/categories", tag = "training", params(PaginationQuery), responses((status = 200, description = "Performance indicator categories", body = PerformanceIndicatorCategoryListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.read")))]
pub async fn list_performance_indicator_categories(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PerformanceIndicatorCategoryListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let total = training_admin_repo::count_pi_categories(pool).await?;
    let rows =
        training_admin_repo::list_pi_categories(pool, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(PerformanceIndicatorCategoryListResponse {
        items: rows,
        pagination: meta,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/training/performance-indicators/categories", tag = "training", request_body = CreatePerformanceIndicatorCategoryRequest, responses((status = 201, description = "Performance indicator category created", body = PerformanceIndicatorCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update")))]
pub async fn create_performance_indicator_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    headers: HeaderMap,
    Json(payload): Json<CreatePerformanceIndicatorCategoryRequest>,
) -> Result<(StatusCode, Json<PerformanceIndicatorCategoryItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = training_admin_repo::insert_pi_category(
        pool,
        &Uuid::new_v4().to_string(),
        &payload.template_id,
        payload.name.trim(),
        payload.sort_order,
    )
    .await?;
    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "PERFORMANCE_INDICATOR_CATEGORY",
        Some(row.id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/performance-indicators/categories/{category_id}", tag = "training", params(("category_id" = String, Path, description = "Category ID")), request_body = UpdatePerformanceIndicatorCategoryRequest, responses((status = 200, description = "Updated performance indicator category", body = PerformanceIndicatorCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Category not found")))]
pub async fn update_performance_indicator_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePerformanceIndicatorCategoryRequest>,
) -> Result<Json<PerformanceIndicatorCategoryItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_pi_category(pool, &category_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = training_admin_repo::update_pi_category_row(
        pool,
        &category_id,
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload.sort_order,
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "PERFORMANCE_INDICATOR_CATEGORY",
        Some(category_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok(Json(row))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/performance-indicators/categories/{category_id}", tag = "training", params(("category_id" = String, Path, description = "Category ID")), responses((status = 200, description = "Deleted performance indicator category", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Category not found")))]
pub async fn delete_performance_indicator_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_pi_category(pool, &category_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    training_admin_repo::delete_pi_category_row(pool, &category_id).await?;
    record_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "PERFORMANCE_INDICATOR_CATEGORY",
        Some(category_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "performance indicator category deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/training/performance-indicators/criteria", tag = "training", params(PaginationQuery), responses((status = 200, description = "Performance indicator criteria", body = PerformanceIndicatorCriteriaListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.read")))]
pub async fn list_performance_indicator_criteria(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PerformanceIndicatorCriteriaListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let total = training_admin_repo::count_pi_criteria(pool).await?;
    let rows = training_admin_repo::list_pi_criteria(pool, pagination.page_size, pagination.offset)
        .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(PerformanceIndicatorCriteriaListResponse {
        items: rows,
        pagination: meta,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/training/performance-indicators/criteria", tag = "training", request_body = CreatePerformanceIndicatorCriteriaRequest, responses((status = 201, description = "Performance indicator criteria created", body = PerformanceIndicatorCriteriaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update")))]
pub async fn create_performance_indicator_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    headers: HeaderMap,
    Json(payload): Json<CreatePerformanceIndicatorCriteriaRequest>,
) -> Result<(StatusCode, Json<PerformanceIndicatorCriteriaItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = training_admin_repo::insert_pi_criteria(
        pool,
        &Uuid::new_v4().to_string(),
        &payload.category_id,
        payload.name.trim(),
        payload.sort_order,
    )
    .await?;
    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "PERFORMANCE_INDICATOR_CRITERIA",
        Some(row.id.clone()),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/performance-indicators/criteria/{criteria_id}", tag = "training", params(("criteria_id" = String, Path, description = "Criteria ID")), request_body = UpdatePerformanceIndicatorCriteriaRequest, responses((status = 200, description = "Updated performance indicator criteria", body = PerformanceIndicatorCriteriaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Criteria not found")))]
pub async fn update_performance_indicator_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(criteria_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePerformanceIndicatorCriteriaRequest>,
) -> Result<Json<PerformanceIndicatorCriteriaItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_pi_criteria(pool, &criteria_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = training_admin_repo::update_pi_criteria_row(
        pool,
        &criteria_id,
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload.sort_order,
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "PERFORMANCE_INDICATOR_CRITERIA",
        Some(criteria_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok(Json(row))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/performance-indicators/criteria/{criteria_id}", tag = "training", params(("criteria_id" = String, Path, description = "Criteria ID")), responses((status = 200, description = "Deleted performance indicator criteria", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Criteria not found")))]
pub async fn delete_performance_indicator_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(criteria_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_pi_criteria(pool, &criteria_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    training_admin_repo::delete_pi_criteria_row(pool, &criteria_id).await?;
    record_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "PERFORMANCE_INDICATOR_CRITERIA",
        Some(criteria_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "performance indicator criteria deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/training/progression-assignments", tag = "training", params(PaginationQuery), responses((status = 200, description = "Progression assignments", body = ProgressionAssignmentListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.read")))]
pub async fn list_progression_assignments(
    State(state): State<AppState>,
    _permission: RequirePermission<TrainingLessonsRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ProgressionAssignmentListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let total = training_admin_repo::count_progression_assignments(pool).await?;
    let rows = training_admin_repo::list_progression_assignments(
        pool,
        pagination.page_size,
        pagination.offset,
    )
    .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        ProgressionAssignmentListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/training/progression-assignments", tag = "training", request_body = CreateProgressionAssignmentRequest, responses((status = 201, description = "Progression assignment created", body = ProgressionAssignmentItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update")))]
pub async fn create_progression_assignment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateProgressionAssignmentRequest>,
) -> Result<(StatusCode, ApiJson<ProgressionAssignmentItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    training_admin_repo::upsert_progression_assignment(
        pool,
        &payload.user_id,
        &payload.progression_id,
        actor.actor_id.as_deref(),
    )
    .await?;
    let row = training_admin_repo::fetch_progression_assignment(pool, &payload.user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    record_audit(
        pool,
        user,
        &headers,
        "UPSERT",
        "TRAINING_PROGRESSION_ASSIGNMENT",
        Some(payload.user_id),
        None,
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/progression-assignments/{user_id}", tag = "training", params(("user_id" = String, Path, description = "User ID")), responses((status = 200, description = "Deleted progression assignment", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.lessons.update"), (status = 404, description = "Progression assignment not found")))]
pub async fn delete_progression_assignment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingLessonsUpdate>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = training_admin_repo::fetch_progression_assignment(pool, &user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    training_admin_repo::delete_progression_assignment_row(pool, &user_id).await?;
    record_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "TRAINING_PROGRESSION_ASSIGNMENT",
        Some(user_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "progression assignment deleted".to_string(),
    }))
}

/// Assembles a controller's progression status (assigned progression + per-step
/// pass state). Shared by the read endpoint and the force-complete response.
async fn build_progression_status(
    pool: &sqlx::PgPool,
    user_id: &str,
) -> Result<ProgressionStatusResponse, ApiError> {
    let Some((progression_id, progression_name, next_progression_id)) =
        training_admin_repo::get_user_progression(pool, user_id).await?
    else {
        return Ok(ProgressionStatusResponse {
            progression_id: None,
            progression_name: None,
            next_progression_id: None,
            steps: Vec::new(),
        });
    };
    let rows =
        training_admin_repo::compute_progression_status_steps(pool, &progression_id, user_id)
            .await?;
    let steps = rows
        .into_iter()
        .map(|r| ProgressionStatusStep {
            step_id: r.step_id,
            lesson_id: r.lesson_id,
            lesson_identifier: r.lesson_identifier,
            lesson_name: r.lesson_name,
            sort_order: r.sort_order,
            optional: r.optional,
            passed: r.passed,
            training_session_id: r.training_session_id,
            session_end: r.session_end,
        })
        .collect();
    Ok(ProgressionStatusResponse {
        progression_id: Some(progression_id),
        progression_name: Some(progression_name),
        next_progression_id,
        steps,
    })
}

#[utoipa::path(get, path = "/api/v1/users/{cid}/progression", tag = "training", params(("cid" = i64, Path, description = "User CID")), responses((status = 200, description = "Progression status", body = ProgressionStatusResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Own progression needs auth.profile.read; someone else's needs training.lessons.read"), (status = 404, description = "User not found")))]
pub async fn get_user_progression_status(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ProgressionStatusResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    // Data-dependent auth (same shape as org.rs::get_user_certifications): self
    // needs only auth.profile.read; viewing someone else needs
    // training.lessons.read.
    if user.cid != cid {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["training", "lessons"], PermissionAction::Read),
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
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let target_user_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status = build_progression_status(pool, &target_user_id).await?;
    Ok(ApiJson::new(status, time))
}

#[utoipa::path(post, path = "/api/v1/users/{cid}/progression/complete", tag = "training", params(("cid" = i64, Path, description = "User CID")), responses((status = 200, description = "Progression advanced (or unchanged)", body = ProgressionStatusResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Own completion needs auth.profile.update, someone else's needs training.lessons.update, or force-finish opted out"), (status = 404, description = "User not found")))]
pub async fn force_complete_progression(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ProgressionStatusResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let is_self = user.cid == cid;
    // Self-completion (the profile "Complete Progression" button) needs only the
    // self-service auth.profile.update; forcing someone else's needs
    // training.lessons.update.
    if is_self {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["auth", "profile"], PermissionAction::Update),
        )
        .await?;
    } else {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["training", "lessons"], PermissionAction::Update),
        )
        .await?;
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let target_user_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Self-service completion honors the user's opt-out flag, matching the
    // website's `noForceProgressionFinish` gate on the Complete button. Staff
    // forcing another controller's progression bypass it.
    if is_self {
        let flags = user_repo::fetch_user_flags(pool, &target_user_id).await?;
        if flags.no_force_progression_finish {
            return Err(ApiError::Forbidden);
        }
    }

    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    advance_progression_if_complete(
        &state,
        pool,
        actor.actor_id.as_deref(),
        &target_user_id,
        false,
        is_self,
    )
    .await?;

    let status = build_progression_status(pool, &target_user_id).await?;
    Ok(ApiJson::new(status, time))
}

/// Advances a controller's progression when its requirements are met, mirroring
/// the website's `assignNextProgressionOrRemove`. Always requires every
/// non-optional step passed; when `require_all_steps` is set (the roster
/// completion sweep) it additionally requires every optional step passed too.
/// On success it either assigns the current progression's `next_progression_id`
/// (emailing `progression.assigned`) or removes the assignment (emailing
/// `progression.removed`). Emails are best-effort. Returns whether the
/// assignment changed. Safe to call from outside a request (roster sync,
/// post-session-save) — it takes an optional actor id and needs no headers.
pub(crate) async fn advance_progression_if_complete(
    state: &AppState,
    pool: &sqlx::PgPool,
    actor_id: Option<&str>,
    user_id: &str,
    require_all_steps: bool,
    user_initiated: bool,
) -> Result<bool, ApiError> {
    let Some((current_id, current_name, next_id)) =
        training_admin_repo::get_user_progression(pool, user_id).await?
    else {
        return Ok(false);
    };
    let steps =
        training_admin_repo::compute_progression_status_steps(pool, &current_id, user_id).await?;
    if !progression_advance_ready(&steps, require_all_steps) {
        return Ok(false);
    }

    let controller_name = fetch_display_name(pool, user_id).await?;

    // The next progression may have been deleted since assignment.
    let next = match next_id.as_deref() {
        Some(next_id) => training_admin_repo::fetch_progression(pool, next_id).await?,
        None => None,
    };

    match next {
        Some(next_prog) => {
            training_admin_repo::upsert_progression_assignment(
                pool,
                user_id,
                &next_prog.id,
                actor_id,
            )
            .await?;
            enqueue_progression_email(
                state,
                pool,
                actor_id,
                user_id,
                "progression.assigned",
                &controller_name,
                &next_prog.name,
            )
            .await;
            record_progression_advance_audit(
                pool,
                actor_id,
                user_id,
                "UPDATE",
                json!({ "from": current_name, "to": next_prog.name, "user_initiated": user_initiated }),
            )
            .await?;
        }
        None => {
            training_admin_repo::delete_progression_assignment_row(pool, user_id).await?;
            enqueue_progression_email(
                state,
                pool,
                actor_id,
                user_id,
                "progression.removed",
                &controller_name,
                &current_name,
            )
            .await;
            record_progression_advance_audit(
                pool,
                actor_id,
                user_id,
                "DELETE",
                json!({ "from": current_name, "user_initiated": user_initiated }),
            )
            .await?;
        }
    }
    Ok(true)
}

/// Whether a progression's steps satisfy the advance rule. Always requires
/// every non-optional step passed; when `require_all_steps` is set (the roster
/// completion sweep) it additionally requires every optional step passed too.
/// An empty progression is never "complete" (avoids silently unassigning a
/// progression that has no steps yet). Mirrors the website's
/// `assignNextProgressionOrRemove` guard (and the stricter roster gate).
fn progression_advance_ready(
    steps: &[training_admin_repo::ProgressionStatusStepRow],
    require_all_steps: bool,
) -> bool {
    if steps.is_empty() {
        return false;
    }
    if steps.iter().any(|s| !s.optional && !s.passed) {
        return false;
    }
    if require_all_steps && steps.iter().any(|s| !s.passed) {
        return false;
    }
    true
}

async fn fetch_display_name(pool: &sqlx::PgPool, user_id: &str) -> Result<String, ApiError> {
    sqlx::query_scalar::<_, String>("select display_name from identity.users where id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
        .map(|name| name.unwrap_or_else(|| "Controller".to_string()))
}

async fn enqueue_progression_email(
    state: &AppState,
    pool: &sqlx::PgPool,
    actor_id: Option<&str>,
    user_id: &str,
    template_id: &str,
    controller_name: &str,
    progression_name: &str,
) {
    let email_actor = EmailActor {
        actor_id: actor_id.map(str::to_string),
        user_id: None,
        service_account_id: None,
        request_source: "system".to_string(),
    };
    let payload = json!({
        "controller_name": controller_name,
        "progression_name": progression_name,
    });
    let _ = state
        .email
        .enqueue_to_users(
            pool,
            email_actor,
            template_id.to_string(),
            payload,
            vec![user_id.to_string()],
        )
        .await;
}

async fn record_progression_advance_audit(
    pool: &sqlx::PgPool,
    actor_id: Option<&str>,
    user_id: &str,
    action: &str,
    after: serde_json::Value,
) -> Result<(), ApiError> {
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor_id.map(str::to_string),
            action: action.to_string(),
            resource_type: "TRAINING_PROGRESSION_ASSIGNMENT".to_string(),
            resource_id: Some(user_id.to_string()),
            scope_type: "training_progression".to_string(),
            scope_key: None,
            message: None,
            before_state: None,
            after_state: Some(audit_repo::sanitize_value(after)),
            ip_address: None,
        },
    )
    .await
}

#[utoipa::path(get, path = "/api/v1/users/{cid}/dossier", tag = "training", params(("cid" = i64, Path, description = "User CID"), PaginationQuery), responses((status = 200, description = "User dossier entries", body = DossierEntryListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Own dossier needs auth.profile.read; someone else's needs training.lessons.read")))]
pub async fn get_user_dossier(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<DossierEntryListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    // Data-dependent authorization: viewing one's own dossier only requires the
    // self-service "auth.profile.read" permission, not the training-admin permission
    // required to view someone else's dossier. Not a single RequirePermission<P> case.
    if user.cid != cid {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["training", "lessons"], PermissionAction::Read),
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
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    // Confidential entries require training.dossier_confidential.read regardless of
    // whether the caller is viewing their own dossier or someone else's — the point
    // of a confidential entry is that the subject isn't necessarily meant to see it.
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    let include_confidential = permissions.contains(&PermissionPath::from_segments(
        ["training", "dossier_confidential"],
        PermissionAction::Read,
    ));
    let pagination = query.resolve(25, 200);
    let total = training_admin_repo::count_dossier_entries(pool, cid, include_confidential).await?;
    let rows = training_admin_repo::list_dossier_entries(
        pool,
        cid,
        include_confidential,
        pagination.page_size,
        pagination.offset,
    )
    .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        DossierEntryListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/{cid}/dossier",
    tag = "training",
    params(("cid" = i64, Path, description = "User CID")),
    request_body = CreateDossierEntryRequest,
    responses(
        (status = 201, description = "Dossier entry created", body = DossierEntryItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.dossier.create"),
        (status = 404, description = "User not found")
    )
)]
pub async fn create_dossier_entry(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingDossierCreate>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateDossierEntryRequest>,
) -> Result<(StatusCode, ApiJson<DossierEntryItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let message = payload.message.trim();
    if message.is_empty() {
        return Err(ApiError::BadRequest);
    }
    let is_confidential = payload.confidential.unwrap_or(false);

    let target_user_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let entry = training_admin_repo::insert_dossier_entry(
        pool,
        &Uuid::new_v4().to_string(),
        &target_user_id,
        &user.id,
        message,
        is_confidential,
    )
    .await?;

    record_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "DOSSIER_ENTRY",
        Some(entry.id.clone()),
        None,
        Some(serde_json::json!({
            "user_id": entry.user_id,
            "is_confidential": entry.is_confidential,
        })),
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(entry, time)))
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
            scope_type: "training_progression".to_string(),
            scope_key: Some(user.cid.to_string()),
            message: None,
            before_state,
            after_state,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::progression_advance_ready;
    use crate::repos::training_admin::ProgressionStatusStepRow;

    fn step(optional: bool, passed: bool) -> ProgressionStatusStepRow {
        ProgressionStatusStepRow {
            step_id: "s".to_string(),
            lesson_id: "l".to_string(),
            lesson_identifier: "GC1".to_string(),
            lesson_name: "Ground".to_string(),
            sort_order: 0,
            optional,
            passed,
            training_session_id: None,
            session_end: None,
        }
    }

    #[test]
    fn empty_progression_is_never_complete() {
        assert!(!progression_advance_ready(&[], false));
        assert!(!progression_advance_ready(&[], true));
    }

    #[test]
    fn all_required_passed_advances_by_default() {
        // required passed, optional unpassed
        let steps = vec![step(false, true), step(true, false)];
        assert!(progression_advance_ready(&steps, false));
    }

    #[test]
    fn unpassed_required_blocks_even_lenient_gate() {
        let steps = vec![step(false, false), step(true, true)];
        assert!(!progression_advance_ready(&steps, false));
        assert!(!progression_advance_ready(&steps, true));
    }

    #[test]
    fn strict_gate_requires_optional_steps_too() {
        // required passed but an optional step is not: the roster sweep (strict)
        // must NOT advance, while the lenient gate (session-save/force) does.
        let steps = vec![step(false, true), step(true, false)];
        assert!(!progression_advance_ready(&steps, true));
        assert!(progression_advance_ready(&steps, false));
    }

    #[test]
    fn everything_passed_advances_under_both_gates() {
        let steps = vec![step(false, true), step(true, true)];
        assert!(progression_advance_ready(&steps, false));
        assert!(progression_advance_ready(&steps, true));
    }
}
