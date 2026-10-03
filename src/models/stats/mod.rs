use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

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
    /// ARTCC-wide hours broken down by month, summed across every controller.
    /// Only populated for the full-year view (`all_time = false`, no `month`
    /// filter) — null otherwise, since a single month or all-time view has
    /// nothing to break down further.
    pub monthly: Option<Vec<MonthlyBucket>>,
}

#[derive(Deserialize, ToSchema)]
pub struct ControllerHistoryQuery {
    pub environment: Option<String>,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct ControllerTotalsQuery {
    pub environment: Option<String>,
    /// Inclusive lower bound (by activation start). When `since` or `until` is
    /// set, totals are summed from individual activations over the window;
    /// otherwise the all-time monthly rollups are used unchanged.
    pub since: Option<DateTime<Utc>>,
    /// Exclusive upper bound (by activation start).
    pub until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct ControllerPositionsQuery {
    pub environment: Option<String>,
    /// Restrict to a single calendar year. Required if `month` is set.
    pub year: Option<i32>,
    /// Restrict to a single month (1-12) within `year`.
    pub month: Option<i32>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct ControllerPositionItem {
    pub position_name: String,
    /// For the primary position, the callsign the controller logged in with
    /// (or the position's configured default for sessions recorded before it
    /// was stored); for a consolidated secondary position, its default callsign.
    pub callsign: Option<String>,
    pub facility_name: String,
    pub is_primary: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub started_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub ended_at: Option<DateTime<Utc>>,
    pub active_seconds: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ControllerPositionListResponse {
    pub items: Vec<ControllerPositionItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
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
pub struct OnlineControllerItem {
    pub cid: i64,
    pub display_name: String,
    pub rating: Option<String>,
    pub position: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OnlineControllersResponse {
    pub items: Vec<OnlineControllerItem>,
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
