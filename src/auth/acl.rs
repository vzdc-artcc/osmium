use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::{errors::ApiError, repos::access as access_repo};

pub const SERVER_ADMIN_ROLE: &str = "SERVER_ADMIN";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Read,
    Create,
    Update,
    Delete,
    Publish,
    Assign,
    Decide,
    Request,
    Approve,
    Deny,
}

impl PermissionAction {
    pub fn from_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read" => Some(Self::Read),
            "create" => Some(Self::Create),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            "publish" => Some(Self::Publish),
            "assign" => Some(Self::Assign),
            "decide" => Some(Self::Decide),
            "request" => Some(Self::Request),
            "approve" => Some(Self::Approve),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }

    pub fn as_value(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Publish => "publish",
            Self::Assign => "assign",
            Self::Decide => "decide",
            Self::Request => "request",
            Self::Approve => "approve",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema)]
pub struct PermissionPath {
    pub segments: Vec<String>,
    pub action: PermissionAction,
}

impl PermissionPath {
    pub fn from_segments<const N: usize>(segments: [&str; N], action: PermissionAction) -> Self {
        Self {
            segments: segments
                .iter()
                .map(|segment| (*segment).to_string())
                .collect(),
            action,
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        let parts: Vec<_> = value.trim().split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        let action = PermissionAction::from_value(parts.last()?)?;
        let segments = parts[..parts.len() - 1]
            .iter()
            .map(|segment| {
                let segment = segment.trim();
                (!segment.is_empty() && is_valid_permission_segment(segment))
                    .then_some(segment.to_string())
            })
            .collect::<Option<Vec<_>>>()?;

        Some(Self { segments, action })
    }

    pub fn as_db_value(&self) -> String {
        let mut parts = self.segments.clone();
        parts.push(self.action.as_value().to_string());
        parts.join(".")
    }
}

fn is_valid_permission_segment(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_lowercase() {
        return false;
    }

    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn default_roles() -> Vec<String> {
    vec![
        SERVER_ADMIN_ROLE.to_string(),
        "USER".to_string(),
        "STAFF".to_string(),
        "ATM".to_string(),
        "DATM".to_string(),
        "TA".to_string(),
        "EC".to_string(),
        "FE".to_string(),
        "WM".to_string(),
        "ATA".to_string(),
        "AWM".to_string(),
        "AEC".to_string(),
        "AFE".to_string(),
        "INS".to_string(),
        "MTR".to_string(),
        "EVENT_STAFF".to_string(),
        "WEB_TEAM".to_string(),
        "BOT".to_string(),
        "SERVICE_APP".to_string(),
    ]
}

fn default_permission_names() -> Vec<String> {
    [
        "auth.profile.read",
        "auth.profile.update",
        "auth.teamspeak_uids.read",
        "auth.teamspeak_uids.create",
        "auth.teamspeak_uids.delete",
        "auth.sessions.delete",
        "auth.dev_login.create",
        "access.self.read",
        "access.catalog.read",
        "access.users.read",
        "access.users.update",
        "api_keys.read",
        "api_keys.create",
        "api_keys.update",
        "api_keys.delete",
        "users.directory.read",
        "users.directory_private.read",
        "users.controller_status.update",
        "users.vatusa_refresh.self.request",
        "users.vatusa_refresh.request",
        "users.visit_artcc.request",
        "users.visitor_applications.self.read",
        "users.visitor_applications.self.request",
        "users.visitor_applications.read",
        "users.visitor_applications.decide",
        "audit.logs.read",
        "training.assignments.read",
        "training.assignments.create",
        "training.ots_recommendations.read",
        "training.ots_recommendations.create",
        "training.ots_recommendations.update",
        "training.ots_recommendations.delete",
        "training.lessons.read",
        "training.lessons.create",
        "training.lessons.update",
        "training.lessons.delete",
        "training.appointments.read",
        "training.appointments.create",
        "training.appointments.update",
        "training.appointments.delete",
        "training.sessions.read",
        "training.sessions.create",
        "training.sessions.update",
        "training.sessions.delete",
        "training.assignment_requests.read",
        "training.assignment_requests.self.request",
        "training.assignment_requests.decide",
        "training.assignment_requests.interest.request",
        "training.assignment_requests.interest.delete",
        "training.release_requests.read",
        "training.release_requests.self.request",
        "training.release_requests.decide",
        "feedback.items.self.read",
        "feedback.items.read",
        "feedback.items.create",
        "feedback.items.decide",
        "events.items.create",
        "events.items.update",
        "events.items.delete",
        "events.positions.self.request",
        "events.positions.assign",
        "events.positions.delete",
        "events.positions.publish",
        "emails.templates.read",
        "emails.preview.create",
        "emails.send.create",
        "emails.outbox.read",
        "emails.suppressions.read",
        "emails.suppressions.update",
        "files.audit.read",
        "files.assets.read",
        "files.assets.create",
        "files.assets.update",
        "files.assets.policy.update",
        "files.assets.delete",
        "files.content.read",
        "files.content.create",
        "files.content.update",
        "files.content.delete",
        "publications.categories.read",
        "publications.categories.create",
        "publications.categories.update",
        "publications.categories.delete",
        "publications.items.read",
        "publications.items.create",
        "publications.items.update",
        "publications.items.delete",
        "stats.artcc.read",
        "stats.controller_events.read",
        "stats.controller_history.read",
        "stats.controller_totals.read",
        "integrations.stats.update",
        "system.read",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub async fn fetch_user_access(
    pool: Option<&PgPool>,
    user_id: &str,
) -> Result<(Vec<String>, Vec<PermissionPath>), ApiError> {
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
) -> Result<(Vec<String>, Vec<PermissionPath>), ApiError> {
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

pub fn permission_tree_from_paths(permissions: &[PermissionPath]) -> Value {
    let mut root = Map::new();

    for permission in permissions {
        insert_permission_path(&mut root, permission);
    }

    Value::Object(root)
}

pub fn permission_tree_from_names(permission_names: &[String]) -> Result<Value, ApiError> {
    let permission_paths = access_repo::permission_names_to_permissions(permission_names.to_vec())?;
    Ok(permission_tree_from_paths(&permission_paths))
}

pub fn normalize_permission_tree(value: &Value) -> Result<Vec<String>, ApiError> {
    let Value::Object(root) = value else {
        return Err(ApiError::BadRequest);
    };

    let mut normalized = BTreeSet::new();
    collect_permission_tree(root, &mut Vec::new(), &mut normalized)?;

    if normalized.is_empty() {
        return Err(ApiError::BadRequest);
    }

    Ok(normalized.into_iter().collect())
}

pub fn is_server_admin(roles: &[String]) -> bool {
    roles.iter().any(|role| role == SERVER_ADMIN_ROLE)
}

fn insert_permission_path(root: &mut Map<String, Value>, permission: &PermissionPath) {
    if permission.segments.is_empty() {
        return;
    }

    let mut node = root;
    for segment in &permission.segments[..permission.segments.len() - 1] {
        let entry = node
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(child) = entry else {
            return;
        };
        node = child;
    }

    let leaf = permission.segments.last().cloned().unwrap_or_default();
    let entry = node.entry(leaf).or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(actions) = entry else {
        return;
    };

    let action = permission.action.as_value();
    if !actions.iter().any(|value| value.as_str() == Some(action)) {
        actions.push(Value::String(action.to_string()));
        actions.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    }
}

fn collect_permission_tree(
    node: &Map<String, Value>,
    path: &mut Vec<String>,
    normalized: &mut BTreeSet<String>,
) -> Result<(), ApiError> {
    for (key, value) in node {
        if !is_valid_permission_segment(key) {
            return Err(ApiError::BadRequest);
        }

        match value {
            Value::Object(child) => {
                path.push(key.clone());
                collect_permission_tree(child, path, normalized)?;
                path.pop();
            }
            Value::Array(actions) => {
                if actions.is_empty() {
                    return Err(ApiError::BadRequest);
                }

                let mut action_set = BTreeSet::new();
                for action_value in actions {
                    let Some(action_name) = action_value.as_str() else {
                        return Err(ApiError::BadRequest);
                    };
                    let action =
                        PermissionAction::from_value(action_name).ok_or(ApiError::BadRequest)?;
                    action_set.insert(action.as_value().to_string());
                }

                let mut segments = path.clone();
                segments.push(key.clone());
                for action in action_set {
                    normalized.insert(format!("{}.{}", segments.join("."), action));
                }
            }
            _ => return Err(ApiError::BadRequest),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        PermissionAction, PermissionPath, SERVER_ADMIN_ROLE, default_permission_names,
        default_roles, filter_assignable_roles, is_server_admin, normalize_permission_tree,
        permission_tree_from_names, permission_tree_from_paths,
    };

    #[test]
    fn parses_permission_from_db_value() {
        assert_eq!(
            PermissionPath::from_db_value("training.appointments.read"),
            Some(PermissionPath::from_segments(
                ["training", "appointments"],
                PermissionAction::Read
            ))
        );
        assert_eq!(PermissionPath::from_db_value("unknown"), None);
        assert_eq!(
            PermissionPath::from_db_value("training.assignment_requests.self.request"),
            Some(PermissionPath::from_segments(
                ["training", "assignment_requests", "self"],
                PermissionAction::Request
            ))
        );
    }

    #[test]
    fn exposes_db_value_names() {
        assert_eq!(
            PermissionPath::from_segments(["files", "content"], PermissionAction::Update)
                .as_db_value(),
            "files.content.update"
        );
    }

    #[test]
    fn has_non_empty_default_access_catalog() {
        assert!(default_roles().contains(&SERVER_ADMIN_ROLE.to_string()));
        assert!(!default_permission_names().is_empty());
    }

    #[test]
    fn default_permission_names_includes_api_keys_perms() {
        let permissions = default_permission_names();
        for required in [
            "api_keys.read",
            "api_keys.create",
            "api_keys.update",
            "api_keys.delete",
        ] {
            assert!(
                permissions.iter().any(|name| name == required),
                "missing default permission: {required}"
            );
        }
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
    fn builds_permission_tree() {
        let tree = permission_tree_from_paths(&[
            PermissionPath::from_segments(["access", "users"], PermissionAction::Update),
            PermissionPath::from_segments(["access", "users"], PermissionAction::Read),
            PermissionPath::from_segments(["files", "assets"], PermissionAction::Create),
        ]);

        assert_eq!(
            tree,
            json!({
                "files": { "assets": ["create"] },
                "access": { "users": ["read", "update"] }
            })
        );
    }

    #[test]
    fn normalizes_nested_permissions() {
        let normalized = normalize_permission_tree(&json!({
            "events": {
                "positions": {
                    "self": ["request"]
                }
            },
            "files": {
                "assets": ["create"]
            }
        }))
        .unwrap();

        assert_eq!(
            normalized,
            vec![
                "events.positions.self.request".to_string(),
                "files.assets.create".to_string()
            ]
        );
    }

    #[test]
    fn rejects_invalid_tree_shape() {
        assert!(normalize_permission_tree(&json!({"events": "read"})).is_err());
    }

    #[test]
    fn builds_tree_from_permission_names() {
        let tree = permission_tree_from_names(&[
            "training.sessions.read".to_string(),
            "training.assignment_requests.self.request".to_string(),
        ])
        .unwrap();

        assert_eq!(
            tree,
            json!({
                "training": {
                    "assignment_requests": {
                        "self": ["request"]
                    },
                    "sessions": ["read"]
                }
            })
        );
    }

    #[test]
    fn detects_server_admin_role() {
        assert!(is_server_admin(&[
            "USER".to_string(),
            SERVER_ADMIN_ROLE.to_string()
        ]));
    }
}
