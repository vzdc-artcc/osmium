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
}

#[derive(Serialize, ToSchema)]
pub struct UserAccessBody {
    pub id: String,
    pub cid: i64,
    pub server_admin: bool,
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
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_id: Option<String>,
    pub actor_type: Option<String>,
    pub scope_type: Option<String>,
    pub scope_key: Option<String>,
    pub action: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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
    pub before_state: Option<serde_json::Value>,
    pub after_state: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
