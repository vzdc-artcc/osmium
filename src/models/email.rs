use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailRecipientsRequest {
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub emails: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailAudienceRequest {
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub artcc: Vec<String>,
    #[serde(default)]
    pub rating: Vec<String>,
    pub receive_event_notifications: Option<bool>,
    pub active_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailSendRequest {
    pub template_id: String,
    #[schema(value_type = Object)]
    pub payload: Value,
    pub recipients: Option<EmailRecipientsRequest>,
    pub audience: Option<EmailAudienceRequest>,
    pub subject_override: Option<String>,
    pub reply_to_address: Option<String>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailPreviewRequest {
    pub template_id: String,
    #[schema(value_type = Object)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailPreviewResponse {
    pub template_id: String,
    pub subject: String,
    pub html: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailSendResponse {
    pub id: Option<String>,
    pub template_id: String,
    pub status: String,
    pub resolved_recipients: usize,
    pub suppressed_recipients: usize,
    pub queued_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailTemplateDefinitionResponse {
    pub id: String,
    pub name: String,
    pub category: String,
    pub is_transactional: bool,
    pub description: String,
    pub allow_arbitrary_addresses: bool,
    #[schema(value_type = Object)]
    pub required_payload_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct EmailOutboxListItem {
    pub id: String,
    pub template_id: String,
    pub category: String,
    pub is_transactional: bool,
    pub request_source: String,
    pub status: String,
    pub attempt_count: i32,
    pub queued_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub recipient_count: i64,
    pub delivered_count: i64,
    pub suppressed_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailOutboxRecipientResponse {
    pub id: String,
    pub user_id: Option<String>,
    pub email: String,
    pub display_name: Option<String>,
    pub source: String,
    pub suppression_reason: Option<String>,
    pub delivery_status: String,
    pub provider_message_id: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailOutboxDetailResponse {
    pub id: String,
    pub template_id: String,
    pub category: String,
    pub is_transactional: bool,
    pub request_source: String,
    pub subject_override: Option<String>,
    pub reply_to_address: Option<String>,
    #[schema(value_type = Object)]
    pub payload: Value,
    #[schema(value_type = Object, nullable = true)]
    pub audience_filter: Option<Value>,
    pub status: String,
    pub attempt_count: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub provider: Option<String>,
    pub provider_message_id: Option<String>,
    pub queued_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub recipients: Vec<EmailOutboxRecipientResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct ListEmailOutboxQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub template_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct EmailPreferencesQuery {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailPreferenceState {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_transactional: bool,
    pub editable: bool,
    pub subscribed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailPreferencesResponse {
    pub email: String,
    pub linked_category: Option<String>,
    pub categories: Vec<EmailPreferenceState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailPreferenceUpdateItem {
    pub category: String,
    pub subscribed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailPreferencesUpdateRequest {
    pub token: String,
    pub preferences: Vec<EmailPreferenceUpdateItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailResubscribeRequest {
    pub category: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EmailSuppressionRecordResponse {
    pub category: String,
    pub email: String,
    pub status: String,
}
