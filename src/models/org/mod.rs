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
pub struct PurgeCandidatesQuery {
    /// Zero-indexed, matching `stats.controller_monthly_rollups.month`.
    pub year: i32,
    pub start_month: i32,
    pub end_month: i32,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListLoasQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub cid: Option<i64>,
    /// Contains-match against the LOA owner's display name.
    pub display_name: Option<String>,
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

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CertificationTypeItem {
    pub id: String,
    pub name: String,
    pub sort_order: i32,
    pub can_solo_cert: bool,
    pub auto_assign_unrestricted: bool,
    /// Allowed `CertificationOption` keys for this type, e.g. `["NONE","DEL","GND"]`.
    pub certification_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CertificationTypeListResponse {
    pub items: Vec<CertificationTypeItem>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RosterCertOption {
    pub certification_type_id: String,
    pub certification_option: String,
}

/// A controller's active solo endorsement for one certification type — carries
/// the position and expiry the roster table shows in the solo-icon tooltip.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RosterSolo {
    pub certification_type_id: String,
    pub position: String,
    pub expires: DateTime<Utc>,
}

/// One controller's roster cert summary: granted (non-`NONE`) certifications,
/// active solo endorsements, and whether they have an approved LOA. Backs the
/// website roster table's cert columns without N+1 per-cid calls.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RosterCertificationItem {
    pub cid: i64,
    pub certifications: Vec<RosterCertOption>,
    pub solos: Vec<RosterSolo>,
    pub has_approved_loa: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RosterCertificationsResponse {
    pub items: Vec<RosterCertificationItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOrUpdateCertificationTypeRequest {
    /// Present = update that type; absent = create a new type.
    pub id: Option<String>,
    pub name: String,
    pub can_solo_cert: bool,
    pub auto_assign_unrestricted: bool,
    /// The full set of allowed `CertificationOption` keys for this type.
    pub certification_options: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CertificationTypeOrderItem {
    pub id: String,
    pub order: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCertificationTypeOrderRequest {
    pub items: Vec<CertificationTypeOrderItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveCertificationEntry {
    pub certification_type_id: String,
    pub certification_option: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveCertificationsRequest {
    pub certifications: Vec<SaveCertificationEntry>,
    /// Free-text dossier note recorded against the controller (required, like the website).
    pub dossier_message: String,
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
    pub email: Option<String>,
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
    /// Contains-match against the submitting user's display name.
    pub display_name: Option<String>,
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

/// Public, unauthenticated shape of a SUA mission — same fields as
/// [`SuaBlockItem`] minus `user_id`, since this is exposed with no auth at
/// all (external clients such as controller plugins poll it).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicSuaMissionItem {
    pub id: String,
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

impl From<SuaBlockItem> for PublicSuaMissionItem {
    fn from(item: SuaBlockItem) -> Self {
        Self {
            id: item.id,
            start_at: item.start_at,
            end_at: item.end_at,
            afiliation: item.afiliation,
            details: item.details,
            mission_number: item.mission_number,
            created_at: item.created_at,
            updated_at: item.updated_at,
            cid: item.cid,
            display_name: item.display_name,
            airspace: item.airspace,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpcomingSuaMissionsResponse {
    pub items: Vec<PublicSuaMissionItem>,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct PurgeCandidateItem {
    pub cid: i64,
    pub display_name: String,
    pub email: String,
    pub rating: Option<String>,
    pub controller_status: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub join_date: DateTime<Utc>,
    pub controlling_hours: f64,
    pub trainer_hours_given: f64,
    pub trainer_hours_received: f64,
    pub total_hours: f64,
    pub open_broadcasts: i64,
    pub has_active_approved_loa: bool,
}

impl From<crate::repos::org::roster_purge::PurgeCandidateRow> for PurgeCandidateItem {
    fn from(row: crate::repos::org::roster_purge::PurgeCandidateRow) -> Self {
        let total_hours = row.controlling_hours + row.trainer_hours_given + row.trainer_hours_received;
        Self {
            cid: row.cid,
            display_name: row.display_name,
            email: row.email,
            rating: row.rating,
            controller_status: row.controller_status,
            join_date: row.join_date,
            controlling_hours: row.controlling_hours,
            trainer_hours_given: row.trainer_hours_given,
            trainer_hours_received: row.trainer_hours_received,
            total_hours,
            open_broadcasts: row.open_broadcasts,
            has_active_approved_loa: row.has_active_approved_loa,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PurgeCandidatesResponse {
    pub items: Vec<PurgeCandidateItem>,
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
