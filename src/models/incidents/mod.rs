use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IncidentItem {
    pub id: String,
    pub reporter_id: String,
    pub reportee_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub timestamp: DateTime<Utc>,
    pub reason: String,
    pub closed: bool,
    pub reporter_callsign: Option<String>,
    pub reportee_callsign: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
    pub reporter_cid: Option<i64>,
    pub reporter_name: Option<String>,
    pub reportee_cid: Option<i64>,
    pub reportee_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateIncidentRequest {
    pub reportee_id: String,
    pub timestamp: DateTime<Utc>,
    pub reason: String,
    pub reporter_callsign: Option<String>,
    pub reportee_callsign: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateIncidentRequest {
    pub closed: bool,
    pub resolution: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListIncidentsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub closed: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IncidentListResponse {
    pub items: Vec<IncidentItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}
