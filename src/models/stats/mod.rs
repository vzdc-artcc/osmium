use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct ArtccStatsQuery {
    pub environment: Option<String>,
    pub all_time: Option<bool>,
    pub month: Option<i32>,
    pub year: Option<i32>,
    pub top: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ArtccStatsResponse {
    pub environment: String,
    pub label: String,
    pub all_time: bool,
    pub month: Option<i32>,
    pub year: Option<i32>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub updated_at: Option<DateTime<Utc>>,
    pub controller_count: i64,
    pub summary: ArtccSummary,
    pub leaders: Vec<ControllerLeader>,
    pub controllers: Vec<ControllerTotals>,
}

#[derive(Deserialize, ToSchema)]
pub struct ControllerHistoryQuery {
    pub environment: Option<String>,
    pub year: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerHistoryResponse {
    pub environment: String,
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub year: i32,
    pub months: Vec<MonthlyBucket>,
}

#[derive(Deserialize, ToSchema)]
pub struct ControllerEventsQuery {
    pub environment: Option<String>,
    pub after_id: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerEventsResponse {
    pub environment: String,
    pub events: Vec<ControllerEventItem>,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerEventItem {
    pub id: i64,
    pub environment: String,
    pub event_type: String,
    pub cid: i64,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub activation_id: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerTotalsResponse {
    pub environment: String,
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub last_activity_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Clone, ToSchema)]
pub struct MonthlyBucket {
    pub month: i32,
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
}

#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct ArtccSummary {
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerLeader {
    pub rank: i32,
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub online_hours: f64,
    pub active_hours: f64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct StatisticsPrefixes {
    pub id: String,
    pub prefixes: Vec<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateStatisticsPrefixesRequest {
    pub prefixes: Vec<String>,
}

#[derive(Serialize, Clone, ToSchema)]
pub struct ControllerTotals {
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
    pub online_hours: f64,
    pub delivery_hours: f64,
    pub ground_hours: f64,
    pub tower_hours: f64,
    pub tracon_hours: f64,
    pub center_hours: f64,
    pub active_hours: f64,
    pub total_hours: f64,
}
