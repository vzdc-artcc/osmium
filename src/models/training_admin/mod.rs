use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingProgressionItem {
    pub id: String,
    pub name: String,
    pub next_progression_id: Option<String>,
    pub auto_assign_new_home_obs: bool,
    pub auto_assign_new_visitor: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingProgressionStepItem {
    pub id: String,
    pub progression_id: String,
    pub lesson_id: String,
    pub sort_order: i32,
    pub optional: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PerformanceIndicatorTemplateItem {
    pub id: String,
    pub name: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgressionAssignmentItem {
    pub user_id: String,
    pub progression_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub assigned_at: DateTime<Utc>,
    pub assigned_by_actor_id: Option<String>,
    pub cid: Option<i64>,
    /// The name the user chose to display, which can be a preferred name.
    pub display_name: Option<String>,
    /// The student's legal name, which staff lists show.
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub progression_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DossierEntryItem {
    pub id: String,
    pub user_id: String,
    pub writer_id: String,
    pub message: String,
    pub is_confidential: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub timestamp: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    pub writer_cid: Option<i64>,
    pub writer_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDossierEntryRequest {
    pub message: String,
    /// Defaults to false. Confidential entries are only visible to callers
    /// with training.dossier_confidential.read.
    pub confidential: Option<bool>,
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
pub struct TrainingProgressionListResponse {
    pub items: Vec<TrainingProgressionItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrainingProgressionStepListResponse {
    pub items: Vec<TrainingProgressionStepItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PerformanceIndicatorTemplateListResponse {
    pub items: Vec<PerformanceIndicatorTemplateItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PerformanceIndicatorCategoryListResponse {
    pub items: Vec<PerformanceIndicatorCategoryItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PerformanceIndicatorCriteriaListResponse {
    pub items: Vec<PerformanceIndicatorCriteriaItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProgressionAssignmentListResponse {
    pub items: Vec<ProgressionAssignmentItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DossierEntryListResponse {
    pub items: Vec<DossierEntryItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

/// One step of a controller's assigned progression, with whether the most
/// recent training ticket for the step's lesson passed. Mirrors the website's
/// `TrainingProgressionStepStatus`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProgressionStatusStep {
    pub step_id: String,
    pub lesson_id: String,
    pub lesson_identifier: String,
    pub lesson_name: String,
    pub sort_order: i32,
    pub optional: bool,
    pub passed: bool,
    /// Id of the most recent training session for this lesson (if any).
    pub training_session_id: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub session_end: Option<DateTime<Utc>>,
}

/// A controller's progression status: the assigned progression (if any) plus
/// per-step pass state.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProgressionStatusResponse {
    pub progression_id: Option<String>,
    pub progression_name: Option<String>,
    pub next_progression_id: Option<String>,
    pub steps: Vec<ProgressionStatusStep>,
}
