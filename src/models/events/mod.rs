use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Event {
    pub id: String,
    pub title: String,
    pub event_type: Option<String>,
    pub host: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub published: bool,
    pub banner_asset_id: Option<String>,
    pub hidden: bool,
    pub positions_locked: bool,
    pub manual_positions_open: bool,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub starts_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventPosition {
    pub id: String,
    pub event_id: String,
    pub callsign: String,
    pub user_id: Option<String>,
    pub user_cid: Option<i64>,
    pub user_name: Option<String>,
    /// Controller's VATSIM rating (e.g. `S3`, `C1`), from the roster membership.
    pub user_rating: Option<String>,
    /// Controller's roster status (`HOME` / `VISITOR` / `NONE`), from the membership.
    pub user_controller_status: Option<String>,
    /// Controller's linked Discord user id, for @mention on event postings.
    pub user_discord_id: Option<String>,
    pub requested_slot: Option<i32>,
    pub assigned_slot: Option<i32>,
    pub requested_position: Option<String>,
    pub requested_secondary_position: String,
    pub notes: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub requested_start_time: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub requested_end_time: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub final_start_time: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub final_end_time: Option<DateTime<Utc>>,
    pub final_position: Option<String>,
    pub final_notes: Option<String>,
    pub controlling_category: Option<String>,
    pub is_instructor: bool,
    pub is_solo: bool,
    pub is_ots: bool,
    pub is_tmu: bool,
    pub is_cic: bool,
    pub published: bool,
    pub status: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct UserEventPositionItem {
    pub id: String,
    pub event_id: String,
    pub event_title: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub event_starts_at: DateTime<Utc>,
    pub event_type: String,
    pub final_position: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub final_start_time: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub final_end_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserEventPositionListResponse {
    pub items: Vec<UserEventPositionItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct EventOpsPlanItem {
    pub id: String,
    pub title: String,
    pub positions_locked: bool,
    pub manual_positions_open: bool,
    pub featured_fields: Vec<String>,
    pub preset_positions: Vec<String>,
    pub featured_field_configs: Option<Value>,
    pub tmis: Option<String>,
    pub ops_free_text: Option<String>,
    pub ops_plan_published: bool,
    pub ops_planner_id: Option<String>,
    pub ops_planner_cid: Option<i64>,
    pub ops_planner_name: Option<String>,
    pub enable_buffer_times: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventTmiItem {
    pub id: String,
    pub event_id: String,
    pub tmi_type: String,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub start_time: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventTmiListResponse {
    pub items: Vec<EventTmiItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEventOpsPlanRequest {
    pub featured_fields: Option<Vec<String>>,
    pub preset_positions: Option<Vec<String>>,
    pub featured_field_configs: Option<Value>,
    pub tmis: Option<Option<String>>,
    pub ops_free_text: Option<Option<String>>,
    pub ops_plan_published: Option<bool>,
    pub ops_planner_id: Option<Option<String>>,
    pub enable_buffer_times: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEventTmiRequest {
    pub tmi_type: String,
    /// Optional legacy scheduled start; omitted for the simplified type+text TMIs.
    pub start_time: Option<DateTime<Utc>>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEventTmiRequest {
    pub tmi_type: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub notes: Option<Option<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePresetPositionsRequest {
    pub preset_positions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateEventRequest {
    pub title: String,
    pub event_type: Option<String>,
    pub host: Option<String>,
    pub description: Option<String>,
    pub banner_asset_id: Option<String>,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateEventRequest {
    pub title: Option<String>,
    pub event_type: Option<String>,
    pub host: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub published: Option<bool>,
    /// Unset (field omitted) leaves the banner untouched; `null` clears it.
    pub banner_asset_id: Option<Option<String>>,
    pub hidden: Option<bool>,
    pub manual_positions_open: Option<bool>,
    /// `true` archives the event (sets `archived_at` if not already set); `false`
    /// un-archives it (clears `archived_at`). Omitted leaves it untouched.
    pub archived: Option<bool>,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateEventPositionRequest {
    pub requested_position: String,
    pub requested_secondary_position: Option<String>,
    pub notes: Option<String>,
    pub requested_start_time: DateTime<Utc>,
    pub requested_end_time: DateTime<Utc>,
    /// Admin-only: create this position on behalf of another user (manual add),
    /// bypassing self-signup. Requires `events.positions.assign`; omit for normal
    /// self-service signup.
    pub user_id: Option<String>,
    /// Admin-only manual-add fields — set the position's final assignment
    /// immediately instead of leaving it as a pending request. Ignored for
    /// self-service signup.
    pub final_position: Option<String>,
    pub final_start_time: Option<DateTime<Utc>>,
    pub final_end_time: Option<DateTime<Utc>>,
    pub final_notes: Option<String>,
    pub controlling_category: Option<String>,
    pub is_instructor: Option<bool>,
    pub is_solo: Option<bool>,
    pub is_ots: Option<bool>,
    pub is_tmu: Option<bool>,
    pub is_cic: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateEventPositionRequest {
    /// Reassign to a different user, or `null` to unassign.
    pub user_id: Option<Option<String>>,
    pub assigned_slot: Option<i32>,
    pub final_position: Option<Option<String>>,
    pub final_start_time: Option<Option<DateTime<Utc>>>,
    pub final_end_time: Option<Option<DateTime<Utc>>>,
    pub final_notes: Option<Option<String>>,
    pub controlling_category: Option<Option<String>>,
    pub is_instructor: Option<bool>,
    pub is_solo: Option<bool>,
    pub is_ots: Option<bool>,
    pub is_tmu: Option<bool>,
    pub is_cic: Option<bool>,
    pub published: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventListResponse {
    pub items: Vec<Event>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventPositionListResponse {
    pub items: Vec<EventPosition>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct EventPositionPreset {
    pub id: String,
    pub name: String,
    pub positions: Vec<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventPositionPresetListResponse {
    pub items: Vec<EventPositionPreset>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEventPositionPresetRequest {
    pub name: String,
    pub positions: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEventPositionPresetRequest {
    pub name: Option<String>,
    pub positions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct OpsPlanFile {
    pub id: String,
    pub event_id: String,
    pub asset_id: Option<String>,
    pub filename: String,
    pub url: Option<String>,
    pub file_type: Option<String>,
    pub uploaded_by: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OpsPlanFileListResponse {
    pub items: Vec<OpsPlanFile>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOpsPlanFileRequest {
    pub asset_id: Option<String>,
    pub filename: String,
    pub url: Option<String>,
    pub file_type: Option<String>,
}
