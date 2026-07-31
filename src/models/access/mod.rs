use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Serialize, ToSchema)]
pub struct AclDebugBody {
    pub user_id: String,
    pub server_admin: bool,
    pub permissions: serde_json::Value,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserAccessRequest {
    pub permissions: serde_json::Value,
    /// Coarse authorization role names to assign, from the assignable set
    /// (STAFF/INS/MTR/EVENT_STAFF — never SERVER_ADMIN, which is only ever
    /// claimed via the OSMIUM_SERVER_ADMIN_CID login path). Omit to leave
    /// roles unchanged. Auto-synced roles (STAFF/INS/MTR, from VATUSA
    /// facility roles) become manually-owned once set here and stop
    /// following roster sync until changed again by a human.
    #[serde(default)]
    pub role_names: Option<Vec<String>>,
    /// Required. Recorded as a dossier entry on the target user's log
    /// alongside the existing USER_ACCESS audit entry.
    pub reason: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserAccessBody {
    pub id: String,
    pub cid: i64,
    pub server_admin: bool,
    pub role_names: Vec<String>,
    pub permissions: serde_json::Value,
}

#[derive(Serialize, ToSchema)]
pub struct AccessCatalogBody {
    pub service_account_roles: Vec<String>,
    pub permissions: serde_json::Value,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceAccountSessionBody {
    pub id: String,
    pub key: String,
    pub name: String,
    pub roles: Vec<String>,
    pub permissions: serde_json::Value,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListAuditLogsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_id: Option<String>,
    pub actor_type: Option<String>,
    pub scope_type: Option<String>,
    pub scope_key: Option<String>,
    pub action: Option<String>,
    /// Comma-separated resource_type allow-list for domain-scoped views (e.g.
    /// `TRAINING_SESSION,TRAINING_APPOINTMENT`). Single string, not repeated keys,
    /// because the handler uses serde_urlencoded which can't build a Vec.
    pub resource_types: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct AuditLogItem {
    pub id: String,
    pub actor_id: Option<String>,
    pub actor_type: Option<String>,
    pub actor_display_name: Option<String>,
    pub actor_user_id: Option<String>,
    pub actor_service_account_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub scope_type: String,
    pub scope_key: Option<String>,
    pub message: Option<String>,
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditLogListResponse {
    pub items: Vec<AuditLogItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyListResponse {
    pub items: Vec<ApiKeyListItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub permissions: serde_json::Value,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateApiKeyRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiKeyListItem {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub prefix: Option<String>,
    pub last_four: Option<String>,
    pub created_by_user_id: Option<String>,
    pub created_by_display_name: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiKeyDetail {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub prefix: Option<String>,
    pub last_four: Option<String>,
    pub created_by_user_id: Option<String>,
    pub created_by_display_name: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub permissions: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub key: ApiKeyDetail,
    /// Plaintext API key secret. Returned exactly once at creation time.
    pub secret: String,
}
