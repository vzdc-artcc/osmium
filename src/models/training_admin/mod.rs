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
    pub display_name: Option<String>,
    pub progression_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DossierEntryItem {
    pub id: String,
    pub user_id: String,
    pub writer_id: String,
    pub message: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub timestamp: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
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
