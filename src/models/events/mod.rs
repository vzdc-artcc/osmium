use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Event {
    pub id: String,
    pub title: String,
    #[sqlx(rename = "event_type")]
    pub event_type: Option<String>,
    pub host: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub published: bool,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct EventPosition {
    pub id: String,
    pub event_id: String,
    pub callsign: String,
    pub user_id: Option<String>,
    pub requested_slot: Option<i32>,
    pub assigned_slot: Option<i32>,
    pub published: bool,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EventTmi {
    pub id: String,
    pub event_id: String,
    pub tmi_type: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub notes: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OpsPlanFile {
    pub id: String,
    pub event_id: String,
    pub filename: String,
    pub url: Option<String>,
    pub file_type: Option<String>,
    pub uploaded_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
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
