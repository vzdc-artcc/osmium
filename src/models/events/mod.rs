use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Event {
    pub id: String,
    pub title: String,
    pub event_type: Option<String>,
    pub host: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub published: bool,
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
    pub requested_slot: Option<i32>,
    pub assigned_slot: Option<i32>,
    pub published: bool,
    pub status: String,
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
    pub enable_buffer_times: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventTmiItem {
    pub id: String,
    pub event_id: String,
    pub tmi_type: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start_time: DateTime<Utc>,
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
    pub start_time: DateTime<Utc>,
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
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateEventPositionRequest {
    pub callsign: String,
    pub requested_slot: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AssignEventPositionRequest {
    pub user_id: String,
    pub assigned_slot: i32,
}

#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct ListEventsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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
