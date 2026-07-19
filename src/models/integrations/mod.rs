use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordConfigItem {
    pub id: String,
    pub name: String,
    pub guild_id: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordChannelItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub channel_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordRoleItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub role_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordCategoryItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub category_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct OutboundJobItem {
    pub id: String,
    pub job_type: String,
    pub subject_type: Option<String>,
    pub subject_id: Option<String>,
    pub status: String,
    pub attempt_count: i32,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub last_attempt_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub payload: Value,
    pub error: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscordLinkStateBody {
    pub linked: bool,
    pub external_id: Option<String>,
    pub auth_url: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordConfigRequest {
    pub name: String,
    pub guild_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordConfigRequest {
    pub name: Option<String>,
    pub guild_id: Option<Option<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordChannelRequest {
    pub discord_config_id: String,
    pub name: String,
    pub channel_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordChannelRequest {
    pub name: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordRoleRequest {
    pub discord_config_id: String,
    pub name: String,
    pub role_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordRoleRequest {
    pub name: Option<String>,
    pub role_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordCategoryRequest {
    pub discord_config_id: String,
    pub name: String,
    pub category_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordCategoryRequest {
    pub name: Option<String>,
    pub category_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AnnouncementRequest {
    pub title: String,
    pub body_markdown: String,
    pub details_url: Option<String>,
    pub send_email: Option<bool>,
    pub send_discord: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EventPublishDiscordRequest {
    pub ping_users: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscordLinkStartRequest {
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscordLinkCompleteRequest {
    pub code: String,
    pub state: String,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscordUnlinkRequest {
    pub external_id: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct OutboundJobsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiscordConfigBundle {
    pub configs: Vec<DiscordConfigItem>,
    pub channels: Vec<DiscordChannelItem>,
    pub roles: Vec<DiscordRoleItem>,
    pub categories: Vec<DiscordCategoryItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OutboundJobListResponse {
    pub items: Vec<OutboundJobItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}
