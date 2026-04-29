use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::{errors::ApiError, repos::access as access_repo};

pub const SERVER_ADMIN_ROLE: &str = "SERVER_ADMIN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionResource {
    Auth,
    Audit,
    System,
    Users,
    Training,
    Feedback,
    Files,
    Events,
    Stats,
    Integrations,
    Web,
}

impl PermissionResource {
    pub fn from_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auth" => Some(Self::Auth),
            "audit" => Some(Self::Audit),
            "system" => Some(Self::System),
            "users" => Some(Self::Users),
            "training" => Some(Self::Training),
            "feedback" => Some(Self::Feedback),
            "files" => Some(Self::Files),
            "events" => Some(Self::Events),
            "stats" => Some(Self::Stats),
            "integrations" => Some(Self::Integrations),
            "web" => Some(Self::Web),
            _ => None,
        }
    }

    pub fn as_value(&self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::Audit => "audit",
            Self::System => "system",
            Self::Users => "users",
            Self::Training => "training",
            Self::Feedback => "feedback",
            Self::Files => "files",
            Self::Events => "events",
            Self::Stats => "stats",
            Self::Integrations => "integrations",
            Self::Web => "web",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Read,
    Create,
    Update,
    Delete,
    Manage,
}

impl PermissionAction {
    pub fn from_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read" => Some(Self::Read),
            "create" => Some(Self::Create),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            "manage" => Some(Self::Manage),
            _ => None,
        }
    }

    pub fn as_value(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Manage => "manage",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub struct PermissionKey {
    pub resource: PermissionResource,
    pub action: PermissionAction,
}

impl PermissionKey {
    pub const fn new(resource: PermissionResource, action: PermissionAction) -> Self {
        Self { resource, action }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        let mut parts = value.trim().split('.');
        let resource = PermissionResource::from_value(parts.next()?)?;
        let action = PermissionAction::from_value(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }

        Some(Self { resource, action })
    }

    pub fn as_db_value(&self) -> String {
        format!("{}.{}", self.resource.as_value(), self.action.as_value())
    }
}

pub type GroupedPermissions = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct PermissionOverrideGroups {
    #[serde(default)]
    pub grant: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub deny: BTreeMap<String, Vec<String>>,
}

const LEGACY_PERMISSION_MAPPINGS: &[(&str, PermissionKey)] = &[
    (
        "read_own_profile",
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Read),
    ),
    (
        "logout",
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Delete),
    ),
    (
        "read_system_readiness",
        PermissionKey::new(PermissionResource::System, PermissionAction::Read),
    ),
    (
        "view_all_users",
        PermissionKey::new(PermissionResource::Users, PermissionAction::Read),
    ),
    (
        "manage_users",
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    ),
    (
        "manage_training",
        PermissionKey::new(PermissionResource::Training, PermissionAction::Manage),
    ),
    (
        "manage_feedback",
        PermissionKey::new(PermissionResource::Feedback, PermissionAction::Update),
    ),
    (
        "upload_files",
        PermissionKey::new(PermissionResource::Files, PermissionAction::Create),
    ),
    (
        "manage_files",
        PermissionKey::new(PermissionResource::Files, PermissionAction::Update),
    ),
    (
        "dev_login_as_cid",
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Manage),
    ),
    (
        "manage_events",
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    ),
    (
        "publish_events",
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update),
    ),
    (
        "manage_stats",
        PermissionKey::new(PermissionResource::Stats, PermissionAction::Manage),
    ),
    (
        "manage_integrations",
        PermissionKey::new(PermissionResource::Integrations, PermissionAction::Manage),
    ),
    (
        "manage_web_content",
        PermissionKey::new(PermissionResource::Web, PermissionAction::Update),
    ),
];

fn default_roles() -> Vec<String> {
    vec![
        SERVER_ADMIN_ROLE.to_string(),
        "USER".to_string(),
        "STAFF".to_string(),
    ]
}

fn default_permission_names() -> Vec<String> {
    vec![
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Read).as_db_value(),
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Delete).as_db_value(),
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Manage).as_db_value(),
        PermissionKey::new(PermissionResource::Audit, PermissionAction::Read).as_db_value(),
        PermissionKey::new(PermissionResource::System, PermissionAction::Read).as_db_value(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Read).as_db_value(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update).as_db_value(),
        PermissionKey::new(PermissionResource::Training, PermissionAction::Read).as_db_value(),
        PermissionKey::new(PermissionResource::Training, PermissionAction::Create).as_db_value(),
        PermissionKey::new(PermissionResource::Training, PermissionAction::Update).as_db_value(),
        PermissionKey::new(PermissionResource::Training, PermissionAction::Manage).as_db_value(),
        PermissionKey::new(PermissionResource::Feedback, PermissionAction::Update).as_db_value(),
        PermissionKey::new(PermissionResource::Files, PermissionAction::Create).as_db_value(),
        PermissionKey::new(PermissionResource::Files, PermissionAction::Read).as_db_value(),
        PermissionKey::new(PermissionResource::Files, PermissionAction::Update).as_db_value(),
        PermissionKey::new(PermissionResource::Files, PermissionAction::Delete).as_db_value(),
        PermissionKey::new(PermissionResource::Events, PermissionAction::Read).as_db_value(),
        PermissionKey::new(PermissionResource::Events, PermissionAction::Create).as_db_value(),
        PermissionKey::new(PermissionResource::Events, PermissionAction::Update).as_db_value(),
        PermissionKey::new(PermissionResource::Events, PermissionAction::Delete).as_db_value(),
        PermissionKey::new(PermissionResource::Stats, PermissionAction::Manage).as_db_value(),
        PermissionKey::new(PermissionResource::Integrations, PermissionAction::Manage)
            .as_db_value(),
        PermissionKey::new(PermissionResource::Web, PermissionAction::Update).as_db_value(),
    ]
}

pub async fn fetch_user_access(
    pool: Option<&PgPool>,
    user_id: &str,
) -> Result<(Vec<String>, Vec<PermissionKey>), ApiError> {
    let Some(pool) = pool else {
        return Ok((Vec::new(), Vec::new()));
    };

    let roles = access_repo::fetch_user_role_names(pool, user_id).await?;
    let permission_names = access_repo::fetch_user_permission_names(pool, user_id).await?;
    let permissions = access_repo::permission_names_to_permissions(permission_names)?;

    Ok((roles, permissions))
}

pub async fn fetch_service_account_access(
    pool: Option<&PgPool>,
    service_account_id: &str,
) -> Result<(Vec<String>, Vec<PermissionKey>), ApiError> {
    let Some(pool) = pool else {
        return Ok((Vec::new(), Vec::new()));
    };

    let roles = access_repo::fetch_service_account_role_names(pool, service_account_id).await?;
    let permission_names =
        access_repo::fetch_service_account_permission_names(pool, service_account_id).await?;
    let permissions = access_repo::permission_names_to_permissions(permission_names)?;

    Ok((roles, permissions))
}

pub async fn fetch_access_catalog(
    pool: Option<&PgPool>,
) -> Result<(Vec<String>, Vec<String>), ApiError> {
    let Some(pool) = pool else {
        return Ok((default_roles(), default_permission_names()));
    };

    let (mut roles, mut permissions) = access_repo::fetch_access_catalog_names(pool).await?;

    if roles.is_empty() {
        roles = default_roles();
    }

    roles = filter_assignable_roles(roles);

    if permissions.is_empty() {
        permissions = default_permission_names();
    }

    Ok((roles, permissions))
}

fn filter_assignable_roles(roles: Vec<String>) -> Vec<String> {
    roles
        .into_iter()
        .filter(|role| role != SERVER_ADMIN_ROLE)
        .collect()
}

pub fn group_permission_keys(permissions: &[PermissionKey]) -> GroupedPermissions {
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for permission in permissions {
        grouped
            .entry(permission.resource.as_value().to_string())
            .or_default()
            .insert(permission.action.as_value().to_string());
    }

    grouped
        .into_iter()
        .map(|(resource, actions)| (resource, actions.into_iter().collect()))
        .collect()
}

pub fn group_permission_names(permission_names: &[String]) -> Result<GroupedPermissions, ApiError> {
    let permission_keys = access_repo::permission_names_to_permissions(permission_names.to_vec())?;
    Ok(group_permission_keys(&permission_keys))
}

pub fn normalize_grouped_permissions(
    grouped: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, ApiError> {
    let mut normalized = BTreeSet::new();

    for (resource_name, actions) in grouped {
        let resource = PermissionResource::from_value(resource_name).ok_or(ApiError::BadRequest)?;
        if actions.is_empty() {
            return Err(ApiError::BadRequest);
        }

        for action_name in actions {
            let action = PermissionAction::from_value(action_name).ok_or(ApiError::BadRequest)?;
            normalized.insert(PermissionKey::new(resource, action).as_db_value());
        }
    }

    if normalized.is_empty() {
        return Err(ApiError::BadRequest);
    }

    Ok(normalized.into_iter().collect())
}

pub fn parse_legacy_permission_name(value: &str) -> Option<PermissionKey> {
    LEGACY_PERMISSION_MAPPINGS
        .iter()
        .find_map(|(legacy, permission)| (*legacy == value.trim()).then_some(*permission))
}

pub fn normalize_legacy_permission_name(value: &str) -> Option<String> {
    parse_legacy_permission_name(value).map(|permission| permission.as_db_value())
}

pub fn normalize_permission_override_groups(
    overrides: &PermissionOverrideGroups,
) -> Result<Vec<(String, bool)>, ApiError> {
    let grant = normalize_grouped_permissions(&overrides.grant).or_else(|error| {
        if overrides.grant.is_empty() {
            Ok(Vec::new())
        } else {
            Err(error)
        }
    })?;
    let deny = normalize_grouped_permissions(&overrides.deny).or_else(|error| {
        if overrides.deny.is_empty() {
            Ok(Vec::new())
        } else {
            Err(error)
        }
    })?;

    let deny_set: BTreeSet<String> = deny.into_iter().collect();
    let mut overrides_out = Vec::new();

    for permission in grant {
        if deny_set.contains(&permission) {
            return Err(ApiError::BadRequest);
        }
        overrides_out.push((permission, true));
    }

    for permission in deny_set {
        overrides_out.push((permission, false));
    }

    if overrides_out.is_empty() {
        return Err(ApiError::BadRequest);
    }

    overrides_out.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    Ok(overrides_out)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        PermissionAction, PermissionKey, PermissionOverrideGroups, PermissionResource,
        SERVER_ADMIN_ROLE, default_permission_names, default_roles, filter_assignable_roles,
        group_permission_keys, normalize_grouped_permissions, normalize_legacy_permission_name,
        normalize_permission_override_groups,
    };

    #[test]
    fn parses_permission_from_db_value() {
        assert_eq!(
            PermissionKey::from_db_value("users.update"),
            Some(PermissionKey::new(
                PermissionResource::Users,
                PermissionAction::Update
            ))
        );
        assert_eq!(PermissionKey::from_db_value("unknown"), None);
    }

    #[test]
    fn exposes_db_value_names() {
        assert_eq!(
            PermissionKey::new(PermissionResource::Auth, PermissionAction::Delete).as_db_value(),
            "auth.delete"
        );
    }

    #[test]
    fn has_non_empty_default_access_catalog() {
        assert_eq!(
            default_roles(),
            vec![
                SERVER_ADMIN_ROLE.to_string(),
                "USER".to_string(),
                "STAFF".to_string()
            ]
        );
        assert!(!default_permission_names().is_empty());
    }

    #[test]
    fn filters_server_admin_from_assignable_roles() {
        let roles = vec![
            "USER".to_string(),
            SERVER_ADMIN_ROLE.to_string(),
            "STAFF".to_string(),
        ];

        assert_eq!(
            filter_assignable_roles(roles),
            vec!["USER".to_string(), "STAFF".to_string()]
        );
    }

    #[test]
    fn groups_permissions_for_api_output() {
        let grouped = group_permission_keys(&[
            PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
            PermissionKey::new(PermissionResource::Users, PermissionAction::Read),
            PermissionKey::new(PermissionResource::Files, PermissionAction::Create),
        ]);

        assert_eq!(
            grouped.get("users").cloned().unwrap_or_default(),
            vec!["read".to_string(), "update".to_string()]
        );
        assert_eq!(
            grouped.get("files").cloned().unwrap_or_default(),
            vec!["create".to_string()]
        );
    }

    #[test]
    fn normalizes_grouped_permissions() {
        let grouped = BTreeMap::from([
            (
                "events".to_string(),
                vec!["update".to_string(), "read".to_string()],
            ),
            ("files".to_string(), vec!["create".to_string()]),
        ]);

        assert_eq!(
            normalize_grouped_permissions(&grouped).unwrap(),
            vec![
                "events.read".to_string(),
                "events.update".to_string(),
                "files.create".to_string()
            ]
        );
    }

    #[test]
    fn normalizes_legacy_permission_names() {
        assert_eq!(
            normalize_legacy_permission_name("manage_users"),
            Some("users.update".to_string())
        );
        assert_eq!(
            normalize_legacy_permission_name("manage_training"),
            Some("training.manage".to_string())
        );
    }

    #[test]
    fn rejects_duplicate_permission_across_grant_and_deny() {
        let overrides = PermissionOverrideGroups {
            grant: BTreeMap::from([("events".to_string(), vec!["update".to_string()])]),
            deny: BTreeMap::from([("events".to_string(), vec!["update".to_string()])]),
        };

        assert!(normalize_permission_override_groups(&overrides).is_err());
    }
}
