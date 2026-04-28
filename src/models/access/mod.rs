use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Serialize, ToSchema)]
pub struct AclDebugBody {
    pub user_id: String,
    pub role: Option<String>,
    pub roles: Vec<String>,
    pub permissions: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateUserAccessRequest {
    pub role: Option<String>,
    pub roles: Option<Vec<String>>,
    pub permissions: Option<PermissionInput>,
    pub permission_overrides: Option<crate::auth::acl::PermissionOverrideGroups>,
}

#[derive(Deserialize, ToSchema)]
#[serde(untagged)]
pub enum PermissionInput {
    Grouped(BTreeMap<String, Vec<String>>),
    Legacy(Vec<PermissionOverrideInput>),
}

#[derive(Deserialize, ToSchema)]
pub struct PermissionOverrideInput {
    pub name: String,
    pub granted: bool,
}

#[derive(Serialize, ToSchema)]
pub struct UserAccessBody {
    pub id: String,
    pub cid: i64,
    pub role: Option<String>,
    pub roles: Vec<String>,
    pub permissions: BTreeMap<String, Vec<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct AccessCatalogBody {
    pub roles: Vec<String>,
    pub permissions: BTreeMap<String, Vec<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceAccountSessionBody {
    pub id: String,
    pub key: String,
    pub name: String,
    pub roles: Vec<String>,
    pub permissions: BTreeMap<String, Vec<String>>,
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
