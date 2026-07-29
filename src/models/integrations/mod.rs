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
    /// Logical Discord config channel to post to; defaults to `announcements`.
    /// Event promos pass `event_announcements`.
    pub channel: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EventPublishDiscordRequest {
    pub ping_users: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordScheduledEventRequest {
    /// External event location text (required by Discord for external events),
    /// e.g. `vatsim.net`. Defaults to `vatsim.net` when omitted.
    pub location: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscordLinkStartRequest {
    /// Website URL to return the browser to once linking completes. osmium 302s
    /// here (with `discord_linked`/`discord_error` query params) after the
    /// server-side token exchange. Honored only if its origin is allowlisted.
    pub return_url: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct DiscordLinkCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
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

/// Canonical list of toggleable Discord bot segments: `(key, human label)`.
/// This is the single source of truth for the Website Management "Bot Features"
/// checklist — add a tuple here (and gate it in the bot) to expose a new toggle.
pub const BOT_FEATURES: &[(&str, &str)] = &[
    ("staffup", "Staffup — live position posts"),
    ("commands", "Slash commands"),
    ("announcements", "Announcement & promo posts"),
    ("event_postings", "Event position postings"),
    ("scheduled_events", "Discord scheduled events"),
    ("audit_log", "Audit logging"),
    ("role_sync", "Role sync"),
    ("break_board", "Break board"),
    ("impromptu_selector", "Impromptu training selector"),
    ("impromptu_offers", "Impromptu session offers"),
];

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BotFeatureFlag {
    pub key: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BotFeatureFlagsResponse {
    pub features: Vec<BotFeatureFlag>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBotFeatureFlagsRequest {
    /// Map of feature key -> enabled. Unknown keys are ignored.
    pub features: std::collections::HashMap<String, bool>,
}

/// A guild the Discord bot is a member of, proxied from the bot for use in
/// configuration UIs so operators pick a guild instead of pasting an id.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredGuild {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredGuildListResponse {
    pub guilds: Vec<DiscoveredGuild>,
}

/// A selectable channel within a guild, proxied live from the Discord bot.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredChannel {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub parent_category_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredCategory {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredRole {
    pub id: String,
    pub name: String,
}

/// Live channel/category/role snapshot for a single guild, proxied from the
/// Discord bot to populate the website's Discord configuration dropdowns.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GuildDiscoveryResponse {
    pub guild_id: String,
    pub channels: Vec<DiscoveredChannel>,
    pub categories: Vec<DiscoveredCategory>,
    pub roles: Vec<DiscoveredRole>,
}
