use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    repos::audit as audit_repo,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingProgressionItem {
    pub id: String,
    pub name: String,
    pub next_progression_id: Option<String>,
    pub auto_assign_new_home_obs: bool,
    pub auto_assign_new_visitor: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingProgressionStepItem {
    pub id: String,
    pub progression_id: String,
    pub lesson_id: String,
    pub sort_order: i32,
    pub optional: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PerformanceIndicatorTemplateItem {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PerformanceIndicatorCategoryItem {
    pub id: String,
    pub template_id: String,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PerformanceIndicatorCriteriaItem {
    pub id: String,
    pub category_id: String,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct ProgressionAssignmentItem {
    pub user_id: String,
    pub progression_id: String,
    pub assigned_at: DateTime<Utc>,
    pub assigned_by_actor_id: Option<String>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
    pub progression_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DossierEntryItem {
    pub id: String,
    pub user_id: String,
    pub writer_id: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub writer_cid: Option<i64>,
    pub writer_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTrainingProgressionRequest {
    pub name: String,
    pub next_progression_id: Option<String>,
    pub auto_assign_new_home_obs: Option<bool>,
    pub auto_assign_new_visitor: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTrainingProgressionRequest {
    pub name: Option<String>,
    pub next_progression_id: Option<Option<String>>,
    pub auto_assign_new_home_obs: Option<bool>,
    pub auto_assign_new_visitor: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTrainingProgressionStepRequest {
    pub progression_id: String,
    pub lesson_id: String,
    pub sort_order: i32,
    pub optional: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTrainingProgressionStepRequest {
    pub lesson_id: Option<String>,
    pub sort_order: Option<i32>,
    pub optional: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePerformanceIndicatorTemplateRequest {
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePerformanceIndicatorTemplateRequest {
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePerformanceIndicatorCategoryRequest {
    pub template_id: String,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePerformanceIndicatorCategoryRequest {
    pub name: Option<String>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePerformanceIndicatorCriteriaRequest {
    pub category_id: String,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePerformanceIndicatorCriteriaRequest {
    pub name: Option<String>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProgressionAssignmentRequest {
    pub user_id: String,
    pub progression_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

#[utoipa::path(get, path = "/api/v1/admin/training/progressions", tag = "training", responses((status = 200, description = "Training progressions", body = [TrainingProgressionItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_progressions(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<TrainingProgressionItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Read).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = sqlx::query_as::<_, TrainingProgressionItem>(
        "select id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at from training.training_progressions order by name asc",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/v1/admin/training/progressions", tag = "training", request_body = CreateTrainingProgressionRequest, responses((status = 201, description = "Training progression created", body = TrainingProgressionItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_progression(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateTrainingProgressionRequest>,
) -> Result<(StatusCode, Json<TrainingProgressionItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = sqlx::query_as::<_, TrainingProgressionItem>(
        r#"
        insert into training.training_progressions (
            id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at
        )
        values ($1, $2, $3, $4, $5, now(), now())
        returning id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(payload.name.trim())
    .bind(payload.next_progression_id.as_deref())
    .bind(payload.auto_assign_new_home_obs.unwrap_or(false))
    .bind(payload.auto_assign_new_visitor.unwrap_or(false))
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)?;
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
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/progressions/{progression_id}", tag = "training", params(("progression_id" = String, Path, description = "Progression ID")), request_body = UpdateTrainingProgressionRequest, responses((status = 200, description = "Updated training progression", body = TrainingProgressionItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_progression(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(progression_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateTrainingProgressionRequest>,
) -> Result<Json<TrainingProgressionItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_progression(pool, &progression_id).await?;
    let row = sqlx::query_as::<_, TrainingProgressionItem>(
        r#"
        update training.training_progressions
        set name = coalesce($2, name),
            next_progression_id = case when $3::bool then $4 else next_progression_id end,
            auto_assign_new_home_obs = coalesce($5, auto_assign_new_home_obs),
            auto_assign_new_visitor = coalesce($6, auto_assign_new_visitor),
            updated_at = now()
        where id = $1
        returning id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at
        "#,
    )
    .bind(&progression_id)
    .bind(payload.name.as_deref().map(str::trim).filter(|v| !v.is_empty()))
    .bind(payload.next_progression_id.is_some())
    .bind(payload.next_progression_id.flatten())
    .bind(payload.auto_assign_new_home_obs)
    .bind(payload.auto_assign_new_visitor)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;
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
    Ok(Json(row))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/progressions/{progression_id}", tag = "training", params(("progression_id" = String, Path, description = "Progression ID")), responses((status = 200, description = "Deleted training progression", body = ApiMessageBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn delete_progression(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(progression_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_progression(pool, &progression_id).await?;
    sqlx::query("delete from training.training_progressions where id = $1")
        .bind(&progression_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
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

#[utoipa::path(get, path = "/api/v1/admin/training/progression-steps", tag = "training", responses((status = 200, description = "Training progression steps", body = [TrainingProgressionStepItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_progression_steps(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<TrainingProgressionStepItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Read).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = sqlx::query_as::<_, TrainingProgressionStepItem>(
        "select id, progression_id, lesson_id, sort_order, optional, created_at from training.training_progression_steps order by progression_id asc, sort_order asc",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/v1/admin/training/progression-steps", tag = "training", request_body = CreateTrainingProgressionStepRequest, responses((status = 201, description = "Training progression step created", body = TrainingProgressionStepItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_progression_step(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateTrainingProgressionStepRequest>,
) -> Result<(StatusCode, Json<TrainingProgressionStepItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = sqlx::query_as::<_, TrainingProgressionStepItem>(
        r#"
        insert into training.training_progression_steps (id, progression_id, lesson_id, sort_order, optional, created_at)
        values ($1, $2, $3, $4, $5, now())
        returning id, progression_id, lesson_id, sort_order, optional, created_at
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&payload.progression_id)
    .bind(&payload.lesson_id)
    .bind(payload.sort_order)
    .bind(payload.optional.unwrap_or(false))
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)?;
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
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/progression-steps/{step_id}", tag = "training", params(("step_id" = String, Path, description = "Progression step ID")), request_body = UpdateTrainingProgressionStepRequest, responses((status = 200, description = "Updated training progression step", body = TrainingProgressionStepItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_progression_step(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(step_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateTrainingProgressionStepRequest>,
) -> Result<Json<TrainingProgressionStepItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_progression_step(pool, &step_id).await?;
    let row = sqlx::query_as::<_, TrainingProgressionStepItem>(
        r#"
        update training.training_progression_steps
        set lesson_id = coalesce($2, lesson_id),
            sort_order = coalesce($3, sort_order),
            optional = coalesce($4, optional)
        where id = $1
        returning id, progression_id, lesson_id, sort_order, optional, created_at
        "#,
    )
    .bind(&step_id)
    .bind(payload.lesson_id.as_deref())
    .bind(payload.sort_order)
    .bind(payload.optional)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;
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
    Ok(Json(row))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/progression-steps/{step_id}", tag = "training", params(("step_id" = String, Path, description = "Progression step ID")), responses((status = 200, description = "Deleted training progression step", body = ApiMessageBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn delete_progression_step(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(step_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_progression_step(pool, &step_id).await?;
    sqlx::query("delete from training.training_progression_steps where id = $1")
        .bind(&step_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
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

#[utoipa::path(get, path = "/api/v1/admin/training/performance-indicators/templates", tag = "training", responses((status = 200, description = "Performance indicator templates", body = [PerformanceIndicatorTemplateItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_performance_indicator_templates(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<PerformanceIndicatorTemplateItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Read).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = sqlx::query_as::<_, PerformanceIndicatorTemplateItem>(
        "select id, name, created_at, updated_at from training.performance_indicator_templates order by name asc",
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/v1/admin/training/performance-indicators/templates", tag = "training", request_body = CreatePerformanceIndicatorTemplateRequest, responses((status = 201, description = "Performance indicator template created", body = PerformanceIndicatorTemplateItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_performance_indicator_template(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreatePerformanceIndicatorTemplateRequest>,
) -> Result<(StatusCode, Json<PerformanceIndicatorTemplateItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = sqlx::query_as::<_, PerformanceIndicatorTemplateItem>(
        "insert into training.performance_indicator_templates (id, name, created_at, updated_at) values ($1, $2, now(), now()) returning id, name, created_at, updated_at",
    ).bind(Uuid::new_v4().to_string()).bind(payload.name.trim()).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
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
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(patch, path = "/api/v1/admin/training/performance-indicators/templates/{template_id}", tag = "training", params(("template_id" = String, Path, description = "Template ID")), request_body = UpdatePerformanceIndicatorTemplateRequest, responses((status = 200, description = "Updated performance indicator template", body = PerformanceIndicatorTemplateItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_performance_indicator_template(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(template_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePerformanceIndicatorTemplateRequest>,
) -> Result<Json<PerformanceIndicatorTemplateItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_pi_template(pool, &template_id).await?;
    let row = sqlx::query_as::<_, PerformanceIndicatorTemplateItem>(
        "update training.performance_indicator_templates set name = $2, updated_at = now() where id = $1 returning id, name, created_at, updated_at",
    ).bind(&template_id).bind(payload.name.trim()).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)?;
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
    Ok(Json(row))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/performance-indicators/templates/{template_id}", tag = "training", params(("template_id" = String, Path, description = "Template ID")), responses((status = 200, description = "Deleted performance indicator template", body = ApiMessageBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn delete_performance_indicator_template(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(template_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_pi_template(pool, &template_id).await?;
    sqlx::query("delete from training.performance_indicator_templates where id = $1")
        .bind(&template_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
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

#[utoipa::path(get, path = "/api/v1/admin/training/performance-indicators/categories", tag = "training", responses((status = 200, description = "Performance indicator categories", body = [PerformanceIndicatorCategoryItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_performance_indicator_categories(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<PerformanceIndicatorCategoryItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Read).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = sqlx::query_as::<_, PerformanceIndicatorCategoryItem>(
        "select id, template_id, name, sort_order from training.performance_indicator_template_categories order by template_id asc, sort_order asc",
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/v1/admin/training/performance-indicators/categories", tag = "training", request_body = CreatePerformanceIndicatorCategoryRequest, responses((status = 201, description = "Performance indicator category created", body = PerformanceIndicatorCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_performance_indicator_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreatePerformanceIndicatorCategoryRequest>,
) -> Result<(StatusCode, Json<PerformanceIndicatorCategoryItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = sqlx::query_as::<_, PerformanceIndicatorCategoryItem>(
        "insert into training.performance_indicator_template_categories (id, template_id, name, sort_order) values ($1, $2, $3, $4) returning id, template_id, name, sort_order",
    ).bind(Uuid::new_v4().to_string()).bind(&payload.template_id).bind(payload.name.trim()).bind(payload.sort_order).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
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

#[utoipa::path(patch, path = "/api/v1/admin/training/performance-indicators/categories/{category_id}", tag = "training", params(("category_id" = String, Path, description = "Category ID")), request_body = UpdatePerformanceIndicatorCategoryRequest, responses((status = 200, description = "Updated performance indicator category", body = PerformanceIndicatorCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_performance_indicator_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePerformanceIndicatorCategoryRequest>,
) -> Result<Json<PerformanceIndicatorCategoryItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_pi_category(pool, &category_id).await?;
    let row = sqlx::query_as::<_, PerformanceIndicatorCategoryItem>(
        "update training.performance_indicator_template_categories set name = coalesce($2, name), sort_order = coalesce($3, sort_order) where id = $1 returning id, template_id, name, sort_order",
    ).bind(&category_id).bind(payload.name.as_deref().map(str::trim).filter(|v| !v.is_empty())).bind(payload.sort_order).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)?;
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

#[utoipa::path(delete, path = "/api/v1/admin/training/performance-indicators/categories/{category_id}", tag = "training", params(("category_id" = String, Path, description = "Category ID")), responses((status = 200, description = "Deleted performance indicator category", body = ApiMessageBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn delete_performance_indicator_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_pi_category(pool, &category_id).await?;
    sqlx::query("delete from training.performance_indicator_template_categories where id = $1")
        .bind(&category_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
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

#[utoipa::path(get, path = "/api/v1/admin/training/performance-indicators/criteria", tag = "training", responses((status = 200, description = "Performance indicator criteria", body = [PerformanceIndicatorCriteriaItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_performance_indicator_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<PerformanceIndicatorCriteriaItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Read).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>(
        "select id, category_id, name, sort_order from training.performance_indicator_template_criteria order by category_id asc, sort_order asc",
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/v1/admin/training/performance-indicators/criteria", tag = "training", request_body = CreatePerformanceIndicatorCriteriaRequest, responses((status = 201, description = "Performance indicator criteria created", body = PerformanceIndicatorCriteriaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_performance_indicator_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreatePerformanceIndicatorCriteriaRequest>,
) -> Result<(StatusCode, Json<PerformanceIndicatorCriteriaItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let row = sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>(
        "insert into training.performance_indicator_template_criteria (id, category_id, name, sort_order) values ($1, $2, $3, $4) returning id, category_id, name, sort_order",
    ).bind(Uuid::new_v4().to_string()).bind(&payload.category_id).bind(payload.name.trim()).bind(payload.sort_order).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
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

#[utoipa::path(patch, path = "/api/v1/admin/training/performance-indicators/criteria/{criteria_id}", tag = "training", params(("criteria_id" = String, Path, description = "Criteria ID")), request_body = UpdatePerformanceIndicatorCriteriaRequest, responses((status = 200, description = "Updated performance indicator criteria", body = PerformanceIndicatorCriteriaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_performance_indicator_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(criteria_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePerformanceIndicatorCriteriaRequest>,
) -> Result<Json<PerformanceIndicatorCriteriaItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_pi_criteria(pool, &criteria_id).await?;
    let row = sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>(
        "update training.performance_indicator_template_criteria set name = coalesce($2, name), sort_order = coalesce($3, sort_order) where id = $1 returning id, category_id, name, sort_order",
    ).bind(&criteria_id).bind(payload.name.as_deref().map(str::trim).filter(|v| !v.is_empty())).bind(payload.sort_order).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)?;
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

#[utoipa::path(delete, path = "/api/v1/admin/training/performance-indicators/criteria/{criteria_id}", tag = "training", params(("criteria_id" = String, Path, description = "Criteria ID")), responses((status = 200, description = "Deleted performance indicator criteria", body = ApiMessageBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn delete_performance_indicator_criteria(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(criteria_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_pi_criteria(pool, &criteria_id).await?;
    sqlx::query("delete from training.performance_indicator_template_criteria where id = $1")
        .bind(&criteria_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
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

#[utoipa::path(get, path = "/api/v1/admin/training/progression-assignments", tag = "training", responses((status = 200, description = "Progression assignments", body = [ProgressionAssignmentItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_progression_assignments(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<ProgressionAssignmentItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Read).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = sqlx::query_as::<_, ProgressionAssignmentItem>(
        r#"
        select
            up.user_id,
            up.progression_id,
            up.assigned_at,
            up.assigned_by_actor_id,
            u.cid,
            u.display_name,
            tp.name as progression_name
        from training.user_progressions up
        join identity.users u on u.id = up.user_id
        join training.training_progressions tp on tp.id = up.progression_id
        order by up.assigned_at desc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/v1/admin/training/progression-assignments", tag = "training", request_body = CreateProgressionAssignmentRequest, responses((status = 201, description = "Progression assignment created", body = ProgressionAssignmentItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_progression_assignment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateProgressionAssignmentRequest>,
) -> Result<(StatusCode, Json<ProgressionAssignmentItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    sqlx::query(
        r#"
        insert into training.user_progressions (user_id, progression_id, assigned_at, assigned_by_actor_id)
        values ($1, $2, now(), $3)
        on conflict (user_id) do update
        set progression_id = excluded.progression_id,
            assigned_at = excluded.assigned_at,
            assigned_by_actor_id = excluded.assigned_by_actor_id
        "#,
    ).bind(&payload.user_id).bind(&payload.progression_id).bind(actor.actor_id.as_deref()).execute(pool).await.map_err(|_| ApiError::BadRequest)?;
    let row = fetch_progression_assignment(pool, &payload.user_id).await?;
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
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(delete, path = "/api/v1/admin/training/progression-assignments/{user_id}", tag = "training", params(("user_id" = String, Path, description = "User ID")), responses((status = 200, description = "Deleted progression assignment", body = ApiMessageBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn delete_progression_assignment(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_training_admin(&state, user, PermissionAction::Update).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_progression_assignment(pool, &user_id).await?;
    sqlx::query("delete from training.user_progressions where user_id = $1")
        .bind(&user_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
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

#[utoipa::path(get, path = "/api/v1/users/{cid}/dossier", tag = "training", params(("cid" = i64, Path, description = "User CID")), responses((status = 200, description = "User dossier entries", body = [DossierEntryItem]), (status = 401, description = "Not authenticated")))]
pub async fn get_user_dossier(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
) -> Result<Json<Vec<DossierEntryItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if user.cid != cid {
        ensure_training_admin(&state, user, PermissionAction::Read).await?;
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
    let rows = sqlx::query_as::<_, DossierEntryItem>(
        r#"
        select
            d.id,
            d.user_id,
            d.writer_id,
            d.message,
            d.timestamp,
            d.created_at,
            u.cid as writer_cid,
            u.display_name as writer_name
        from feedback.dossier_entries d
        join identity.users target on target.id = d.user_id
        join identity.users u on u.id = d.writer_id
        where target.cid = $1
        order by d.timestamp desc, d.created_at desc
        "#,
    )
    .bind(cid)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

async fn ensure_training_admin(
    state: &AppState,
    user: &CurrentUser,
    action: PermissionAction,
) -> Result<(), ApiError> {
    let permission = match action {
        PermissionAction::Read => {
            PermissionPath::from_segments(["training", "lessons"], PermissionAction::Read)
        }
        _ => PermissionPath::from_segments(["training", "lessons"], PermissionAction::Update),
    };
    ensure_permission(state, Some(user), None, permission).await
}

async fn fetch_progression(
    pool: &sqlx::PgPool,
    progression_id: &str,
) -> Result<TrainingProgressionItem, ApiError> {
    sqlx::query_as::<_, TrainingProgressionItem>("select id, name, next_progression_id, auto_assign_new_home_obs, auto_assign_new_visitor, created_at, updated_at from training.training_progressions where id = $1")
        .bind(progression_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)
}
async fn fetch_progression_step(
    pool: &sqlx::PgPool,
    step_id: &str,
) -> Result<TrainingProgressionStepItem, ApiError> {
    sqlx::query_as::<_, TrainingProgressionStepItem>("select id, progression_id, lesson_id, sort_order, optional, created_at from training.training_progression_steps where id = $1")
        .bind(step_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)
}
async fn fetch_pi_template(
    pool: &sqlx::PgPool,
    template_id: &str,
) -> Result<PerformanceIndicatorTemplateItem, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorTemplateItem>("select id, name, created_at, updated_at from training.performance_indicator_templates where id = $1")
        .bind(template_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)
}
async fn fetch_pi_category(
    pool: &sqlx::PgPool,
    category_id: &str,
) -> Result<PerformanceIndicatorCategoryItem, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCategoryItem>("select id, template_id, name, sort_order from training.performance_indicator_template_categories where id = $1")
        .bind(category_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)
}
async fn fetch_pi_criteria(
    pool: &sqlx::PgPool,
    criteria_id: &str,
) -> Result<PerformanceIndicatorCriteriaItem, ApiError> {
    sqlx::query_as::<_, PerformanceIndicatorCriteriaItem>("select id, category_id, name, sort_order from training.performance_indicator_template_criteria where id = $1")
        .bind(criteria_id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)
}
async fn fetch_progression_assignment(
    pool: &sqlx::PgPool,
    user_id: &str,
) -> Result<ProgressionAssignmentItem, ApiError> {
    sqlx::query_as::<_, ProgressionAssignmentItem>(
        r#"
        select
            up.user_id,
            up.progression_id,
            up.assigned_at,
            up.assigned_by_actor_id,
            u.cid,
            u.display_name,
            tp.name as progression_name
        from training.user_progressions up
        join identity.users u on u.id = up.user_id
        join training.training_progressions tp on tp.id = up.progression_id
        where up.user_id = $1
        "#,
    )
    .bind(user_id)
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
            scope_type: "training_progression".to_string(),
            scope_key: Some(user.cid.to_string()),
            before_state,
            after_state,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}
