use std::collections::{BTreeSet, HashMap};

use serde::Serialize;
use sqlx::PgPool;

use crate::errors::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Role {
    User,
    Staff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadOwnProfile,
    Logout,
    ReadSystemReadiness,
    ViewAllUsers,
    ManageUsers,
    ManageTraining,
    DevLoginAsCid,
}

impl Role {
    pub fn from_db_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "USER" => Some(Self::User),
            "STAFF" => Some(Self::Staff),
            _ => None,
        }
    }
}

impl Permission {
    pub fn from_db_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read_own_profile" => Some(Self::ReadOwnProfile),
            "logout" => Some(Self::Logout),
            "read_system_readiness" => Some(Self::ReadSystemReadiness),
            "view_all_users" => Some(Self::ViewAllUsers),
            "manage_users" => Some(Self::ManageUsers),
            "manage_training" => Some(Self::ManageTraining),
            "dev_login_as_cid" => Some(Self::DevLoginAsCid),
            _ => None,
        }
    }

    pub fn as_db_value(&self) -> &'static str {
        match self {
            Self::ReadOwnProfile => "read_own_profile",
            Self::Logout => "logout",
            Self::ReadSystemReadiness => "read_system_readiness",
            Self::ViewAllUsers => "view_all_users",
            Self::ManageUsers => "manage_users",
            Self::ManageTraining => "manage_training",
            Self::DevLoginAsCid => "dev_login_as_cid",
        }
    }
}

pub fn default_roles() -> Vec<String> {
    vec!["USER".to_string(), "STAFF".to_string()]
}

pub fn default_permission_names() -> Vec<String> {
    vec![
        Permission::ReadOwnProfile.as_db_value().to_string(),
        Permission::Logout.as_db_value().to_string(),
        Permission::ReadSystemReadiness.as_db_value().to_string(),
        Permission::ViewAllUsers.as_db_value().to_string(),
        Permission::ManageUsers.as_db_value().to_string(),
        Permission::ManageTraining.as_db_value().to_string(),
        Permission::DevLoginAsCid.as_db_value().to_string(),
    ]
}

pub fn role_has_permission(role: &str, permission: Permission) -> bool {
    let Some(role) = Role::from_db_value(role) else {
        return false;
    };

    match role {
        Role::User => matches!(permission, Permission::ReadOwnProfile | Permission::Logout),
        Role::Staff => matches!(
            permission,
            Permission::ReadOwnProfile
                | Permission::Logout
                | Permission::ReadSystemReadiness
                | Permission::ViewAllUsers
                | Permission::ManageUsers
                | Permission::ManageTraining
                | Permission::DevLoginAsCid
        ),
    }
}

pub fn permissions_for_role(role: &str) -> Vec<Permission> {
    let all_permissions = [
        Permission::ReadOwnProfile,
        Permission::Logout,
        Permission::ReadSystemReadiness,
        Permission::ViewAllUsers,
        Permission::ManageUsers,
        Permission::ManageTraining,
        Permission::DevLoginAsCid,
    ];

    all_permissions
        .into_iter()
        .filter(|permission| role_has_permission(role, *permission))
        .collect()
}

pub fn effective_permissions_from_roles(roles: &[String]) -> Vec<Permission> {
    let mut collected = BTreeSet::new();
    for role in roles {
        for permission in permissions_for_role(role) {
            collected.insert(permission.as_db_value());
        }
    }

    collected
        .into_iter()
        .filter_map(Permission::from_db_value)
        .collect()
}

fn resolve_effective_permissions(
    role_permission_names: Vec<String>,
    user_permission_overrides: Vec<(String, bool)>,
) -> Vec<Permission> {
    let mut effective = BTreeSet::new();

    for name in role_permission_names {
        effective.insert(name);
    }

    let mut overrides = HashMap::new();
    for (name, granted) in user_permission_overrides {
        overrides.insert(name, granted);
    }

    for (name, granted) in overrides {
        if granted {
            effective.insert(name);
        } else {
            effective.remove(&name);
        }
    }

    effective
        .into_iter()
        .filter_map(|name| Permission::from_db_value(&name))
        .collect()
}

pub async fn fetch_user_access(
    pool: Option<&PgPool>,
    user_id: &str,
    fallback_role: &str,
) -> Result<(Vec<String>, Vec<Permission>), ApiError> {
    let Some(pool) = pool else {
        let roles = vec![fallback_role.to_string()];
        let permissions = effective_permissions_from_roles(&roles);
        return Ok((roles, permissions));
    };

    let mut roles = sqlx::query_scalar::<_, String>(
        "select role_name from user_roles where user_id = $1 order by role_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    if roles.is_empty() {
        roles.push(fallback_role.to_string());
    }

    let role_permissions = sqlx::query_scalar::<_, String>(
        r#"
        select distinct rp.permission_name
        from user_roles ur
        join role_permissions rp on rp.role_name = ur.role_name
        where ur.user_id = $1
        order by permission_name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let user_overrides = sqlx::query_as::<_, (String, bool)>(
        r#"
        select permission_name, granted
        from user_permissions
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let permissions = if role_permissions.is_empty() && user_overrides.is_empty() {
        effective_permissions_from_roles(&roles)
    } else {
        resolve_effective_permissions(role_permissions, user_overrides)
    };

    Ok((roles, permissions))
}

pub async fn fetch_access_catalog(pool: Option<&PgPool>) -> Result<(Vec<String>, Vec<String>), ApiError> {
    let Some(pool) = pool else {
        return Ok((default_roles(), default_permission_names()));
    };

    let mut roles = sqlx::query_scalar::<_, String>("select name from roles order by name")
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    let mut permissions = sqlx::query_scalar::<_, String>("select name from permissions order by name")
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    if roles.is_empty() {
        roles = default_roles();
    }

    if permissions.is_empty() {
        permissions = default_permission_names();
    }

    Ok((roles, permissions))
}

#[cfg(test)]
mod tests {
    use super::{
        Permission, Role, default_permission_names, default_roles, permissions_for_role,
        resolve_effective_permissions, role_has_permission,
    };

    #[test]
    fn parses_role_from_db_value() {
        assert_eq!(Role::from_db_value("USER"), Some(Role::User));
        assert_eq!(Role::from_db_value(" staff "), Some(Role::Staff));
        assert_eq!(Role::from_db_value("admin"), None);
    }

    #[test]
    fn enforces_user_permissions() {
        assert!(role_has_permission("USER", Permission::ReadOwnProfile));
        assert!(role_has_permission("USER", Permission::Logout));
        assert!(!role_has_permission("USER", Permission::ManageUsers));
        assert!(!role_has_permission("USER", Permission::ViewAllUsers));
        assert!(!role_has_permission("USER", Permission::ManageTraining));
        assert!(!role_has_permission("USER", Permission::DevLoginAsCid));
    }

    #[test]
    fn enforces_staff_permissions() {
        assert!(role_has_permission("STAFF", Permission::ReadOwnProfile));
        assert!(role_has_permission("STAFF", Permission::ViewAllUsers));
        assert!(role_has_permission("STAFF", Permission::ManageUsers));
        assert!(role_has_permission("STAFF", Permission::ManageTraining));
        assert!(role_has_permission("STAFF", Permission::DevLoginAsCid));
    }

    #[test]
    fn unknown_role_has_no_permissions() {
        assert!(permissions_for_role("UNKNOWN").is_empty());
    }

    #[test]
    fn deny_override_takes_precedence_over_role_permission() {
        let effective = resolve_effective_permissions(
            vec!["manage_users".to_string(), "logout".to_string()],
            vec![("manage_users".to_string(), false)],
        );

        assert!(!effective.contains(&Permission::ManageUsers));
        assert!(effective.contains(&Permission::Logout));
    }

    #[test]
    fn has_non_empty_default_access_catalog() {
        assert!(!default_roles().is_empty());
        assert!(!default_permission_names().is_empty());
    }
}

