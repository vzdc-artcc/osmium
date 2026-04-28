use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
};

use crate::{
    auth::{
        acl::{
            PermissionAction, PermissionKey, PermissionOverrideGroups, PermissionResource,
            fetch_access_catalog, fetch_user_access, group_permission_keys, group_permission_names,
            normalize_grouped_permissions, normalize_legacy_permission_name,
            normalize_permission_override_groups,
        },
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{
        AccessCatalogBody, AclDebugBody, AdminUserListItem, AuditLogItem, ListAuditLogsQuery,
        ListUsersQuery, PermissionInput, SetControllerStatusBody, SetControllerStatusRequest,
        UpdateUserAccessRequest, UserAccessBody, UserOverviewBody,
    },
    repos::{access as access_repo, audit as audit_repo, users as user_repo},
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/api/v1/admin/acl",
    tag = "admin",
    responses(
        (status = 200, description = "Effective access for the current staff user", body = AclDebugBody),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn acl_debug(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<AclDebugBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    )
    .await?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;

    Ok(Json(AclDebugBody {
        user_id: user.id.clone(),
        role: user.primary_role.clone(),
        roles,
        permissions: group_permission_keys(&permissions),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users/{cid}/access",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    responses(
        (status = 200, description = "Access details for a user", body = UserAccessBody),
        (status = 400, description = "Invalid CID"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_user_access(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
) -> Result<Json<UserAccessBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &target.id).await?;
    Ok(Json(build_user_access_body(&target, roles, permissions)))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/access/catalog",
    tag = "admin",
    responses(
        (status = 200, description = "Assignable roles and permissions", body = AccessCatalogBody),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_access_catalog(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<AccessCatalogBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    )
    .await?;

    let (roles, permissions) = fetch_access_catalog(state.db.as_ref()).await?;
    Ok(Json(AccessCatalogBody {
        roles,
        permissions: group_permission_names(&permissions)?,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/audit",
    tag = "admin",
    params(ListAuditLogsQuery),
    responses(
        (status = 200, description = "Audit log rows", body = [AuditLogItem]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_audit_logs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Query(query): Query<ListAuditLogsQuery>,
) -> Result<Json<Vec<AuditLogItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Audit, PermissionAction::Read),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = audit_repo::fetch_audit_logs(
        pool,
        &audit_repo::AuditLogFilters {
            resource_type: query.resource_type,
            resource_id: query.resource_id,
            actor_id: query.actor_id,
            actor_type: query.actor_type,
            scope_type: query.scope_type,
            scope_key: query.scope_key,
            action: query.action.map(|value| value.trim().to_ascii_uppercase()),
            limit: query.limit.unwrap_or(50).clamp(1, 250),
            offset: query.offset.unwrap_or(0).max(0),
        },
    )
    .await?;

    Ok(Json(rows))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{cid}/controller-status",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    request_body = SetControllerStatusRequest,
    responses(
        (status = 200, description = "Updated controller status", body = SetControllerStatusBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn set_user_controller_status(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<SetControllerStatusRequest>,
) -> Result<Json<SetControllerStatusBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let normalized_status = match payload
        .controller_status
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "HOME" => "HOME",
        "VISITOR" => "VISITOR",
        "NONE" => "NONE",
        _ => return Err(ApiError::BadRequest),
    };

    let normalized_artcc = payload
        .artcc
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase());

    let before = user_repo::find_roster_user_by_cid(pool, cid).await?;
    let updated = user_repo::update_controller_status(
        pool,
        cid,
        normalized_status,
        normalized_artcc.as_deref(),
    )
    .await?
    .ok_or(ApiError::BadRequest)?;

    let response = SetControllerStatusBody {
        cid: updated.0,
        controller_status: updated.1,
        artcc: updated.2,
    };

    let actor =
        audit_repo::resolve_audit_actor(pool, Some(user), current_service_account.as_ref()).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "USER_CONTROLLER_STATUS".to_string(),
            resource_id: before.as_ref().map(|row| row.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(cid.to_string()),
            before_state: before
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: Some(audit_repo::sanitized_snapshot(&response)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(response))
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<Vec<AdminUserListItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let users = user_repo::list_admin_users(pool, limit, offset).await?;

    Ok(Json(users))
}

pub async fn get_user_overview(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
) -> Result<Json<UserOverviewBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target = user_repo::find_admin_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &target.id).await?;
    let stats = user_repo::fetch_user_stats(pool, &target.id).await?;

    Ok(Json(UserOverviewBody {
        user: target,
        roles,
        permissions: group_permission_keys(&permissions),
        stats,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{cid}/access",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    request_body = UpdateUserAccessRequest,
    responses(
        (status = 200, description = "Updated user access", body = UserAccessBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_user_access(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<UpdateUserAccessRequest>,
) -> Result<Json<UserAccessBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionKey::new(PermissionResource::Users, PermissionAction::Update),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let parsed_roles = parse_roles(payload.roles.as_deref(), payload.role.as_deref())?;
    let parsed_permissions = parse_permissions(
        payload.permissions.as_ref(),
        payload.permission_overrides.as_ref(),
    )?;

    if parsed_roles.is_empty() && parsed_permissions.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let target_user_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let target_before = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let (before_roles, before_permissions) =
        fetch_user_access(state.db.as_ref(), &target_before.id).await?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    access_repo::replace_user_access(&mut tx, &target_user_id, &parsed_roles, &parsed_permissions)
        .await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let updated = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &updated.id).await?;
    let response = build_user_access_body(&updated, roles, permissions);
    let actor =
        audit_repo::resolve_audit_actor(pool, Some(user), current_service_account.as_ref()).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "USER_ACCESS".to_string(),
            resource_id: Some(updated.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(cid.to_string()),
            before_state: Some(audit_repo::sanitize_value(serde_json::json!({
                "user": target_before,
                "roles": before_roles,
                "permissions": group_permission_keys(&before_permissions),
            }))),
            after_state: Some(audit_repo::sanitized_snapshot(&response)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(response))
}

fn parse_roles(
    raw_roles: Option<&[String]>,
    raw_role: Option<&str>,
) -> Result<Vec<String>, ApiError> {
    let roles: Vec<String> = if let Some(roles) = raw_roles {
        roles.to_vec()
    } else if let Some(role) = raw_role {
        vec![role.to_string()]
    } else {
        Vec::new()
    };

    if roles.is_empty() {
        return Ok(Vec::new());
    }

    let mut parsed = Vec::with_capacity(roles.len());
    for role in roles {
        let normalized = match role.trim().to_ascii_uppercase().as_str() {
            "USER" => "USER",
            "STAFF" => "STAFF",
            _ => return Err(ApiError::BadRequest),
        };
        if !parsed.iter().any(|value| value == normalized) {
            parsed.push(normalized.to_string());
        }
    }

    Ok(parsed)
}

fn parse_permissions(
    raw_permissions: Option<&PermissionInput>,
    raw_overrides: Option<&PermissionOverrideGroups>,
) -> Result<Vec<(String, bool)>, ApiError> {
    if let Some(overrides) = raw_overrides {
        return normalize_permission_override_groups(overrides);
    }

    let Some(raw_permissions) = raw_permissions else {
        return Ok(Vec::new());
    };

    match raw_permissions {
        PermissionInput::Grouped(grouped) => Ok(normalize_grouped_permissions(grouped)?
            .into_iter()
            .map(|permission| (permission, true))
            .collect()),
        PermissionInput::Legacy(raw_permissions) => {
            let mut parsed: Vec<(String, bool)> = Vec::with_capacity(raw_permissions.len());
            for override_input in raw_permissions {
                let Some(normalized) = normalize_legacy_permission_name(&override_input.name)
                    .or_else(|| {
                        PermissionKey::from_db_value(&override_input.name)
                            .map(|permission| permission.as_db_value())
                    })
                else {
                    return Err(ApiError::BadRequest);
                };

                if let Some(existing) = parsed.iter_mut().find(|value| value.0 == normalized) {
                    existing.1 = override_input.granted;
                } else {
                    parsed.push((normalized, override_input.granted));
                }
            }

            Ok(parsed)
        }
    }
}

fn build_user_access_body(
    user: &CurrentUser,
    roles: Vec<String>,
    permissions: Vec<PermissionKey>,
) -> UserAccessBody {
    UserAccessBody {
        id: user.id.clone(),
        cid: user.cid,
        role: user.primary_role.clone(),
        roles,
        permissions: group_permission_keys(&permissions),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::parse_permissions;
    use crate::{
        auth::acl::PermissionOverrideGroups,
        models::{PermissionInput, PermissionOverrideInput},
    };

    #[test]
    fn parses_grouped_permissions_as_grants() {
        let grouped = PermissionInput::Grouped(BTreeMap::from([(
            "events".to_string(),
            vec!["update".to_string(), "read".to_string()],
        )]));

        assert_eq!(
            parse_permissions(Some(&grouped), None).unwrap(),
            vec![
                ("events.read".to_string(), true),
                ("events.update".to_string(), true)
            ]
        );
    }

    #[test]
    fn parses_legacy_permissions_for_compatibility() {
        let legacy = PermissionInput::Legacy(vec![PermissionOverrideInput {
            name: "manage_users".to_string(),
            granted: true,
        }]);

        assert_eq!(
            parse_permissions(Some(&legacy), None).unwrap(),
            vec![("users.update".to_string(), true)]
        );
    }

    #[test]
    fn parses_explicit_grant_and_deny_overrides() {
        let overrides = PermissionOverrideGroups {
            grant: BTreeMap::from([("files".to_string(), vec!["create".to_string()])]),
            deny: BTreeMap::from([("users".to_string(), vec!["update".to_string()])]),
        };

        assert_eq!(
            parse_permissions(None, Some(&overrides)).unwrap(),
            vec![
                ("files.create".to_string(), true),
                ("users.update".to_string(), false)
            ]
        );
    }
}
