use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
