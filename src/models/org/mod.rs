use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoaItem {
    pub id: String,
    pub user_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub end: DateTime<Utc>,
    pub reason: String,
    pub status: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub submitted_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by_actor_id: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLoaRequest {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateLoaRequest {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideLoaRequest {
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListLoasQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoaListResponse {
    pub items: Vec<LoaItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct CertificationItem {
    pub certification_type_id: String,
    pub certification_type_name: String,
    pub sort_order: i32,
    pub certification_option: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CertificationListResponse {
    pub items: Vec<CertificationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SoloCertificationItem {
    pub id: String,
    pub user_id: String,
    pub certification_type_id: String,
    pub position: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub expires: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub granted_at: DateTime<Utc>,
    pub granted_by_actor_id: Option<String>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
    pub certification_type_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSoloCertificationRequest {
    pub user_id: String,
    pub certification_type_id: String,
    pub position: String,
    pub expires: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSoloCertificationRequest {
    pub certification_type_id: Option<String>,
    pub position: Option<String>,
    pub expires: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListSoloCertificationsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SoloCertificationListResponse {
    pub items: Vec<SoloCertificationItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct StaffingRequestItem {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateStaffingRequestRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListStaffingRequestsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffingRequestListResponse {
    pub items: Vec<StaffingRequestItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SuaAirspaceItem {
    pub id: String,
    pub sua_block_id: String,
    pub identifier: String,
    pub bottom_altitude: String,
    pub top_altitude: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuaBlockItem {
    pub id: String,
    pub user_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub end_at: DateTime<Utc>,
    pub afiliation: String,
    pub details: String,
    pub mission_number: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
    pub airspace: Vec<SuaAirspaceItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSuaAirspaceRequest {
    pub identifier: String,
    pub bottom_altitude: String,
    pub top_altitude: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSuaRequest {
    pub afiliation: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub details: String,
    pub airspace: Vec<CreateSuaAirspaceRequest>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListSuaQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SuaListResponse {
    pub items: Vec<SuaBlockItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ControllerLifecycleRequest {
    pub controller_status: String,
    pub artcc: Option<String>,
    pub cleanup_on_none: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ControllerLifecycleCleanupSummary {
    pub training_assignment_requests_deleted: i64,
    pub training_assignments_deleted: i64,
    pub loas_deleted: i64,
    pub operating_initials_assigned: bool,
    pub operating_initials_cleared: bool,
    pub welcome_message_enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ControllerLifecycleResponse {
    pub cid: i64,
    pub controller_status: String,
    pub artcc: Option<String>,
    pub cleanup: ControllerLifecycleCleanupSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct JobRunItem {
    pub id: String,
    pub job_name: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub started_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub result_summary: Option<Value>,
    pub error_text: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobStatusItem {
    pub job_name: String,
    pub enabled: bool,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub last_started_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub last_finished_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_result_ok: Option<bool>,
    pub last_error: Option<String>,
    pub latest_run: Option<JobRunItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobDetailResponse {
    pub status: JobStatusItem,
    pub recent_runs: Vec<JobRunItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobRunResponse {
    pub run: JobRunItem,
}
