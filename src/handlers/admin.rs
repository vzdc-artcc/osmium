use std::collections::BTreeSet;

use crate::{
    auth::{
        acl::{
            PermissionPath, fetch_access_catalog, fetch_user_access, is_server_admin,
            normalize_permission_tree, permission_tree_from_names, permission_tree_from_paths,
        },
        context::{CurrentServiceAccount, CurrentUser},
        permissions::{
            AccessCatalogRead, AccessSelfRead, AccessUsersRead, AccessUsersUpdate, AuditLogsRead,
            UsersControllerStatusUpdate, UsersDirectoryPrivateRead, UsersFlagsRead,
            UsersFlagsUpdate, UsersOperatingInitialsUpdate, UsersSessionsDelete, UsersSessionsRead,
            UsersStaffPositionsUpdate, UsersVatusaRefreshRequest, UsersVisitorApplicationsDecide,
            UsersVisitorApplicationsRead,
        },
        require_permission::RequirePermission,
    },
    errors::ApiError,
    jobs::roster_sync,
    models::{
        AccessCatalogBody, AclDebugBody, AdminUpdateProfileRequest, AdminUserListResponse,
        AuditLogListResponse, DecideVisitorApplicationRequest, ListAuditLogsQuery,
        ListVisitorApplicationsQuery,
        ManualVatusaRefreshResponse as ManualVatusaRefreshResponseBody,
        ManualVatusaRefreshResult as ManualVatusaRefreshResultBody, MeProfileBody, PaginationMeta,
        PaginationQuery, SetControllerStatusBody, SetControllerStatusRequest,
        StaffPositionsResponse, UpdateOperatingInitialsRequest, UpdateOperatingInitialsResponse,
        UpdateUserAccessRequest, UpdateUserFlagsRequest, UserAccessBody, UserFlagsBody,
        UserOverviewBody, UserSessionListResponse, VisitorApplicationItem,
        VisitorApplicationListResponse,
    },
    repos::{
        access as access_repo, audit as audit_repo, ip_request_log as ip_log_repo,
        users as user_repo,
    },
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
};

const DEFAULT_VATUSA_API_BASE_URL: &str = "https://api.vatusa.net/v2";
const DEFAULT_VATUSA_FACILITY_ID: &str = "ZDC";
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
    _permission: RequirePermission<AccessSelfRead>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<AclDebugBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;

    Ok(Json(AclDebugBody {
        user_id: user.id.clone(),
        server_admin: is_server_admin(&roles),
        permissions: permission_tree_from_paths(&permissions),
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user_access(
    State(state): State<AppState>,
    _permission: RequirePermission<AccessUsersRead>,
    Path(cid): Path<i64>,
) -> Result<Json<UserAccessBody>, ApiError> {
    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &target.id).await?;
    Ok(Json(build_user_access_body(&target, &roles, permissions)))
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
    _permission: RequirePermission<AccessCatalogRead>,
) -> Result<Json<AccessCatalogBody>, ApiError> {
    let (roles, permissions) = fetch_access_catalog(state.db.as_ref()).await?;
    Ok(Json(AccessCatalogBody {
        service_account_roles: roles,
        permissions: permission_tree_from_names(&permissions)?,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/audit",
    tag = "admin",
    params(ListAuditLogsQuery),
    responses(
        (status = 200, description = "Audit log rows", body = AuditLogListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_audit_logs(
    State(state): State<AppState>,
    _permission: RequirePermission<AuditLogsRead>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListAuditLogsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<AuditLogListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    // Only SERVER_ADMIN readers may see server-level impersonation audit rows.
    // Facility admins hold `audit.logs.read` (via the old `audit.read` remap) but
    // must never see `AUTH_IMPERSONATION` activity (security checklist #2).
    let (roles, _) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    let include_server_sensitive = is_server_admin(&roles);

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 250);
    let normalized_action = query
        .action
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase());

    // Free-text resource_type search: trim + uppercase so it matches the stored
    // uppercase values regardless of how the user typed it (mirrors `action`).
    let normalized_resource_type = query
        .resource_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase());

    // Comma-separated domain allow-list (e.g. `TRAINING_SESSION,LESSON`). Split,
    // trim, uppercase, drop empties; None when absent or all-empty so it's a no-op.
    let resource_types = query
        .resource_types
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_uppercase())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty());

    let filters = audit_repo::AuditLogFilters {
        resource_type: normalized_resource_type,
        resource_types,
        resource_id: query.resource_id,
        actor_id: query.actor_id,
        actor_type: query.actor_type,
        scope_type: query.scope_type,
        scope_key: query.scope_key,
        action: normalized_action,
        limit: pagination.page_size,
        offset: pagination.offset,
        include_server_sensitive,
    };

    let total = audit_repo::count_audit_logs(pool, &filters).await?;
    let rows = audit_repo::fetch_audit_logs(pool, &filters).await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        AuditLogListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn set_user_controller_status(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<SetControllerStatusRequest>,
) -> Result<Json<SetControllerStatusBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

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
    .ok_or(ApiError::NotFound)?;

    let response = SetControllerStatusBody {
        cid: updated.0,
        controller_status: updated.1,
        artcc: updated.2,
    };
    let after = user_repo::find_roster_user_by_cid(pool, cid).await?;

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
            message: None,
            before_state: before
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: after
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users/{cid}/flags",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    responses(
        (status = 200, description = "User's self-service opt-out flags", body = UserFlagsBody),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user_flags(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersFlagsRead>,
    Path(cid): Path<i64>,
) -> Result<Json<UserFlagsBody>, ApiError> {
    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(user_repo::fetch_user_flags(pool, &target_id).await?))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{cid}/flags",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    request_body = UpdateUserFlagsRequest,
    responses(
        (status = 200, description = "Updated opt-out flags", body = UserFlagsBody),
        (status = 400, description = "Invalid request (empty reason)"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn update_user_flags(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersFlagsUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<UpdateUserFlagsRequest>,
) -> Result<Json<UserFlagsBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let reason = payload.reason.trim();
    if reason.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let before = user_repo::fetch_user_flags(pool, &target_id).await?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    user_repo::upsert_user_flags(pool, &target_id, &payload).await?;
    access_repo::insert_access_dossier_entry(&mut tx, &target_id, &user.id, reason).await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let after = user_repo::fetch_user_flags(pool, &target_id).await?;

    let actor =
        audit_repo::resolve_audit_actor(pool, Some(user), current_service_account.as_ref()).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "USER_FLAGS".to_string(),
            resource_id: Some(target_id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(cid.to_string()),
            message: None,
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(after))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{cid}/profile",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    request_body = AdminUpdateProfileRequest,
    responses(
        (status = 200, description = "Updated profile", body = MeProfileBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "User not found")
    )
)]
// Reuses the admin user-management gate (users.flags.update, granted to STAFF
// in migration 0052) — same capability class as the flags + OI-reassignment
// admin surfaces this sits next to. Operating initials keep their dedicated
// endpoint; flags keep theirs.
pub async fn admin_update_user_profile(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersFlagsUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<AdminUpdateProfileRequest>,
) -> Result<Json<MeProfileBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    if payload.timezone.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }

    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let before = user_repo::fetch_me_profile(pool, &target_id).await?;
    let profile = user_repo::admin_update_user_profile(
        pool,
        &target_id,
        payload.preferred_name.as_deref(),
        payload.bio.as_deref(),
        payload.timezone.trim(),
    )
    .await?;

    let actor =
        audit_repo::resolve_audit_actor(pool, Some(user), current_service_account.as_ref()).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "USER_PROFILE".to_string(),
            resource_id: Some(target_id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(cid.to_string()),
            message: None,
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&profile)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(profile))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/users/{cid}/operating-initials",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    request_body = UpdateOperatingInitialsRequest,
    responses(
        (status = 200, description = "Reassigned operating initials", body = UpdateOperatingInitialsResponse),
        (status = 400, description = "Invalid request, or initials already in use"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn reassign_user_operating_initials(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersOperatingInitialsUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<UpdateOperatingInitialsRequest>,
) -> Result<Json<UpdateOperatingInitialsResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let initials = payload.operating_initials.trim().to_ascii_uppercase();
    if initials.len() != 2 || !initials.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ApiError::BadRequest);
    }

    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let before = user_repo::fetch_me_profile(pool, &target_id).await?;
    let assigned = user_repo::reassign_operating_initials(pool, &target_id, &initials).await?;
    if !assigned {
        return Err(ApiError::Conflict);
    }
    let after = user_repo::fetch_me_profile(pool, &target_id).await?;

    let response = UpdateOperatingInitialsResponse {
        cid,
        operating_initials: initials,
    };

    let actor =
        audit_repo::resolve_audit_actor(pool, Some(user), current_service_account.as_ref()).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "USER_OPERATING_INITIALS".to_string(),
            resource_id: Some(target_id),
            scope_type: "global".to_string(),
            scope_key: Some(cid.to_string()),
            message: None,
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{cid}/staff-positions/{position}",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        ("position" = String, Path, description = "Staff position code, e.g. ATM, INS, AWM")
    ),
    responses(
        (status = 200, description = "Staff positions after the change", body = StaffPositionsResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn assign_staff_position(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersStaffPositionsUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path((cid, position)): Path<(i64, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<StaffPositionsResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    set_staff_position_for_cid(
        &state,
        user,
        current_service_account.as_ref(),
        cid,
        &position,
        true,
        &headers,
        time,
    )
    .await
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/users/{cid}/staff-positions/{position}",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        ("position" = String, Path, description = "Staff position code, e.g. ATM, INS, AWM")
    ),
    responses(
        (status = 200, description = "Staff positions after the change", body = StaffPositionsResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn revoke_staff_position(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersStaffPositionsUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path((cid, position)): Path<(i64, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<StaffPositionsResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    set_staff_position_for_cid(
        &state,
        user,
        current_service_account.as_ref(),
        cid,
        &position,
        false,
        &headers,
        time,
    )
    .await
}

async fn set_staff_position_for_cid(
    state: &AppState,
    user: &CurrentUser,
    current_service_account: Option<&CurrentServiceAccount>,
    cid: i64,
    position: &str,
    held: bool,
    headers: &HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<StaffPositionsResponse>, ApiError> {
    let position = position.trim().to_ascii_uppercase();
    if !crate::models::STAFF_POSITIONS.contains(&position.as_str()) {
        return Err(ApiError::BadRequest);
    }

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let (target_user_id, _) = user_repo::find_user_identity_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let before = user_repo::list_held_staff_positions(pool, cid).await?;
    user_repo::set_staff_position_manual(pool, &target_user_id, &position, held, &user.id).await?;
    let after = user_repo::list_held_staff_positions(pool, cid).await?;
    let response = StaffPositionsResponse {
        cid,
        positions: after,
    };

    let actor = audit_repo::resolve_audit_actor(pool, Some(user), current_service_account).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: if held { "ASSIGN" } else { "REVOKE" }.to_string(),
            resource_type: "USER_STAFF_POSITION".to_string(),
            resource_id: Some(target_user_id),
            scope_type: "global".to_string(),
            scope_key: Some(format!("{cid}:{position}")),
            message: None,
            before_state: Some(audit_repo::sanitized_snapshot(&StaffPositionsResponse {
                cid,
                positions: before,
            })?),
            after_state: Some(audit_repo::sanitized_snapshot(&response)?),
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await?;

    Ok(ApiJson::new(response, time))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{cid}/refresh-vatusa",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    responses(
        (status = 200, description = "User refreshed from VATUSA", body = ManualVatusaRefreshResponseBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 503, description = "VATUSA or database unavailable")
    )
)]
pub async fn refresh_user_vatusa(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersVatusaRefreshRequest>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<ManualVatusaRefreshResponseBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = user_repo::find_roster_user_by_cid(pool, cid).await?;
    let refreshed = roster_sync::refresh_single_user_from_vatusa(pool, cid).await?;
    let response = ManualVatusaRefreshResponseBody {
        user: crate::handlers::users::build_user_details_response(
            &state,
            Some(user),
            refreshed.user.clone(),
        )
        .await?,
        refresh_result: ManualVatusaRefreshResultBody {
            cid: refreshed.cid,
            membership_outcome: match refreshed.membership_outcome {
                roster_sync::ManualVatusaRefreshOutcome::Home => {
                    crate::models::ManualVatusaRefreshOutcome::Home
                }
                roster_sync::ManualVatusaRefreshOutcome::Visitor => {
                    crate::models::ManualVatusaRefreshOutcome::Visitor
                }
                roster_sync::ManualVatusaRefreshOutcome::OffRoster => {
                    crate::models::ManualVatusaRefreshOutcome::OffRoster
                }
            },
            detail_refreshed: refreshed.detail_refreshed,
            membership_updated: refreshed.membership_updated,
            message: refreshed.message.clone(),
        },
    };

    let after = user_repo::find_roster_user_by_cid(pool, cid).await?;
    let actor =
        audit_repo::resolve_audit_actor(pool, Some(user), current_service_account.as_ref()).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "USER_VATUSA_REFRESH".to_string(),
            resource_id: before.as_ref().map(|row| row.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(cid.to_string()),
            message: None,
            before_state: before
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: after
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(ApiJson::new(response, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/visitor-applications",
    tag = "admin",
    params(ListVisitorApplicationsQuery),
    responses(
        (status = 200, description = "Visitor applications", body = VisitorApplicationListResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_visitor_applications(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersVisitorApplicationsRead>,
    Query(query): Query<ListVisitorApplicationsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<VisitorApplicationListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let normalized_status = normalize_visitor_application_status_filter(query.status.as_deref())?;
    let display_name = query.display_name.as_deref();
    let home_facility = query.home_facility.as_deref();
    let total = user_repo::count_visitor_applications(
        pool,
        normalized_status.as_deref(),
        query.cid,
        display_name,
        home_facility,
    )
    .await?;
    let items = user_repo::list_visitor_applications(
        pool,
        normalized_status.as_deref(),
        query.cid,
        display_name,
        home_facility,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        VisitorApplicationListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/visitor-applications/{application_id}",
    tag = "admin",
    params(
        ("application_id" = String, Path, description = "Visitor application id")
    ),
    request_body = DecideVisitorApplicationRequest,
    responses(
        (status = 200, description = "Visitor application updated", body = VisitorApplicationItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Visitor application or user not found")
    )
)]
pub async fn decide_visitor_application(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersVisitorApplicationsDecide>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(application_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<DecideVisitorApplicationRequest>,
) -> Result<ApiJson<VisitorApplicationItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let normalized_status = normalize_visitor_application_decision_status(&payload.status)?;
    let normalized_reason = payload
        .reason_for_denial
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if normalized_status == "DENIED" && normalized_reason.is_none() {
        return Err(ApiError::BadRequest);
    }

    let before = user_repo::find_visitor_application_by_id(pool, &application_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if normalized_status == "APPROVED" {
        sync_approved_visitor_to_vatusa(before.cid.ok_or(ApiError::BadRequest)?).await?;
    }

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let actor =
        audit_repo::resolve_audit_actor(&mut *tx, Some(user), current_service_account.as_ref())
            .await?;
    let after = user_repo::decide_visitor_application(
        &mut tx,
        &application_id,
        normalized_status,
        if normalized_status == "DENIED" {
            normalized_reason.as_deref()
        } else {
            None
        },
        actor.actor_id.as_deref(),
        &configured_artcc(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;

    audit_repo::record_audit(
        &mut *tx,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "VISITOR_APPLICATION".to_string(),
            resource_id: Some(after.id.clone()),
            scope_type: "global".to_string(),
            scope_key: after.cid.map(|cid| cid.to_string()),
            message: None,
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    // Notify the applicant of the decision (accepted / rejected). Best-effort — a
    // mail failure must not fail the decision.
    let mut email_payload = serde_json::json!({
        "user_name": after
            .display_name
            .clone()
            .unwrap_or_else(|| "Controller".to_string()),
        "artcc_name": configured_artcc(),
    });
    let template_id = if normalized_status == "APPROVED" {
        "visitor.accepted"
    } else {
        if let Some(reason) = &normalized_reason {
            email_payload["reason"] = serde_json::Value::String(reason.clone());
        }
        "visitor.rejected"
    };
    if let Err(error) = state
        .email
        .enqueue_to_users(
            pool,
            crate::email::service::EmailActor {
                actor_id: None,
                user_id: None,
                service_account_id: None,
                request_source: "system".to_string(),
            },
            template_id.to_string(),
            email_payload,
            vec![after.user_id.clone()],
        )
        .await
    {
        tracing::warn!(
            ?error,
            template_id,
            user_id = %after.user_id,
            "failed to enqueue visitor-decision email"
        );
    }

    Ok(ApiJson::new(after, time))
}

pub async fn list_users(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersDirectoryPrivateRead>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<AdminUserListResponse>, ApiError> {
    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = user_repo::count_admin_users(pool).await?;
    let users = user_repo::list_admin_users(pool, pagination.page_size, pagination.offset).await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(Json(AdminUserListResponse {
        items: users,
        pagination: meta,
    }))
}

pub async fn get_user_overview(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersDirectoryPrivateRead>,
    Path(cid): Path<i64>,
) -> Result<Json<UserOverviewBody>, ApiError> {
    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target = user_repo::find_admin_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &target.id).await?;
    let stats = user_repo::fetch_user_stats(pool, &target.id).await?;

    Ok(Json(UserOverviewBody {
        user: target,
        roles,
        permissions: permission_tree_from_paths(&permissions),
        stats,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users/{cid}/ip-history",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        PaginationQuery
    ),
    responses(
        (status = 200, description = "A user's durable request IP history (spec 011)", body = crate::repos::ip_request_log::IpRequestLogListResponse),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user_ip_history(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersDirectoryPrivateRead>,
    Path(cid): Path<i64>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<ip_log_repo::IpRequestLogListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 200);

    let total = ip_log_repo::count_for_user(pool, &target_id).await?;
    let items =
        ip_log_repo::list_for_user(pool, &target_id, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        ip_log_repo::IpRequestLogListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/users/{cid}/sessions",
    tag = "admin",
    params(("cid" = i64, Path, description = "VATSIM CID")),
    responses(
        (status = 200, description = "A user's active auth sessions (metadata only, no tokens)", body = UserSessionListResponse),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn list_user_sessions(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersSessionsRead>,
    Path(cid): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<UserSessionListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let items = access_repo::list_user_sessions(pool, &target_id).await?;
    Ok(ApiJson::new(UserSessionListResponse { items }, time))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/users/{cid}/sessions/{session_id}",
    tag = "admin",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        ("session_id" = String, Path, description = "Session id to revoke")
    ),
    responses(
        (status = 200, description = "Session revoked; returns the remaining active sessions", body = UserSessionListResponse),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User or session not found")
    )
)]
pub async fn revoke_user_session(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersSessionsDelete>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path((cid, session_id)): Path<(i64, String)>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<UserSessionListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    let before = access_repo::list_user_sessions(pool, &target_id)
        .await?
        .into_iter()
        .find(|session| session.id == session_id);
    if !access_repo::revoke_user_session(pool, &target_id, &session_id).await? {
        return Err(ApiError::NotFound);
    }

    record_session_revoke_audit(
        pool,
        &headers,
        current_user.as_ref(),
        current_service_account.as_ref(),
        "REVOKE",
        cid,
        before
            .as_ref()
            .map(audit_repo::sanitized_snapshot)
            .transpose()?,
        serde_json::json!({ "revoked_session_id": session_id, "target_cid": cid }),
    )
    .await?;

    let items = access_repo::list_user_sessions(pool, &target_id).await?;
    Ok(ApiJson::new(UserSessionListResponse { items }, time))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/users/{cid}/sessions",
    tag = "admin",
    params(("cid" = i64, Path, description = "VATSIM CID")),
    responses(
        (status = 200, description = "All the user's sessions revoked; returns the (now empty) active list", body = UserSessionListResponse),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn revoke_all_user_sessions(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersSessionsDelete>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<UserSessionListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let target_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Snapshot from the session listing, which carries only non-secret metadata
    // (id, ip, user agent, timestamps) and never the session token.
    let before = access_repo::list_user_sessions(pool, &target_id).await?;
    let revoked = access_repo::revoke_all_user_sessions(pool, &target_id).await?;

    record_session_revoke_audit(
        pool,
        &headers,
        current_user.as_ref(),
        current_service_account.as_ref(),
        "REVOKE_ALL",
        cid,
        Some(audit_repo::sanitized_snapshot(&before)?),
        serde_json::json!({ "revoked_count": revoked, "target_cid": cid }),
    )
    .await?;

    let items = access_repo::list_user_sessions(pool, &target_id).await?;
    Ok(ApiJson::new(UserSessionListResponse { items }, time))
}

/// Records a `USER_SESSION` audit entry for a session revoke (single or all).
async fn record_session_revoke_audit(
    pool: &sqlx::PgPool,
    headers: &HeaderMap,
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
    action: &str,
    target_cid: i64,
    before_state: Option<serde_json::Value>,
    after_state: serde_json::Value,
) -> Result<(), ApiError> {
    let actor =
        audit_repo::resolve_audit_actor(pool, current_user, current_service_account).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.to_string(),
            resource_type: "USER_SESSION".to_string(),
            resource_id: Some(target_cid.to_string()),
            scope_type: "global".to_string(),
            scope_key: Some(target_cid.to_string()),
            message: None,
            before_state,
            after_state: Some(after_state),
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}

fn normalize_visitor_application_status_filter(
    value: Option<&str>,
) -> Result<Option<String>, ApiError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .map_or(Ok(None), |normalized| match normalized.as_str() {
            "PENDING" | "APPROVED" | "DENIED" => Ok(Some(normalized)),
            _ => Err(ApiError::BadRequest),
        })
}

fn normalize_visitor_application_decision_status(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "APPROVED" => Ok("APPROVED"),
        "DENIED" => Ok("DENIED"),
        _ => Err(ApiError::BadRequest),
    }
}

fn configured_artcc() -> String {
    std::env::var("VATUSA_FACILITY_ID")
        .ok()
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_VATUSA_FACILITY_ID.to_string())
}

async fn sync_approved_visitor_to_vatusa(cid: i64) -> Result<(), ApiError> {
    let api_key = std::env::var("VATUSA_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::ServiceUnavailable)?;
    let facility_id = configured_artcc();
    let api_base_url = std::env::var("VATUSA_API_BASE_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_VATUSA_API_BASE_URL.to_string());

    let url = format!(
        "{}/facility/{}/roster/manageVisitor/{}",
        api_base_url, facility_id, cid
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| ApiError::Internal)?;

    let response = client
        .post(&url)
        .query(&[("apikey", api_key.as_str())])
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(cid, %url, ?error, "vatusa manageVisitor request failed");
            ApiError::ServiceUnavailable
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::warn!(cid, %url, %status, body, "vatusa manageVisitor returned non-success status");
        return Err(ApiError::ServiceUnavailable);
    }

    Ok(())
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "User not found")
    )
)]
pub async fn update_user_access(
    State(state): State<AppState>,
    _permission: RequirePermission<AccessUsersUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<UpdateUserAccessRequest>,
) -> Result<Json<UserAccessBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let reason = payload.reason.trim();
    if reason.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let parsed_permissions = parse_permissions(&payload.permissions)?;
    let requested_permissions =
        access_repo::permission_names_to_permissions(parsed_permissions.clone())?;

    if let Some(role_names) = payload.role_names.as_ref() {
        for role_name in role_names {
            if !access_repo::ASSIGNABLE_USER_ROLES.contains(&role_name.as_str()) {
                return Err(ApiError::BadRequest);
            }
        }
    }

    let target_user_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let target_before = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let (before_roles, before_permissions) =
        fetch_user_access(state.db.as_ref(), &target_before.id).await?;
    let before = build_user_access_body(&target_before, &before_roles, before_permissions);

    let existing_direct_names =
        access_repo::fetch_user_direct_permission_names(pool, &target_user_id).await?;
    let existing_direct_permissions =
        access_repo::permission_names_to_permissions(existing_direct_names)?;

    validate_permission_changes_are_within_actor_scope(
        &state,
        user,
        &existing_direct_permissions,
        &requested_permissions,
    )
    .await?;

    // A non-SERVER_ADMIN actor may only grant/revoke a coarse role they
    // themselves effectively hold — same self-scope principle
    // validate_permission_changes_are_within_actor_scope already applies to
    // fine-grained permission changes, extended to roles.
    if let Some(role_names) = payload.role_names.as_ref() {
        let (actor_roles, _) = fetch_user_access(state.db.as_ref(), &user.id).await?;
        if !is_server_admin(&actor_roles) {
            for role_name in role_names {
                if !actor_roles.contains(role_name) {
                    return Err(ApiError::Forbidden);
                }
            }
        }
    }

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    access_repo::replace_user_permissions(&mut tx, &target_user_id, &parsed_permissions).await?;
    if let Some(role_names) = payload.role_names.as_ref() {
        for role_name in access_repo::ASSIGNABLE_USER_ROLES {
            let held = role_names.iter().any(|r| r == role_name);
            access_repo::set_user_role_manual(&mut tx, &target_user_id, role_name, held, &user.id)
                .await?;
        }
    }
    access_repo::insert_access_dossier_entry(&mut tx, &target_user_id, &user.id, reason).await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let updated = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &updated.id).await?;
    let response = build_user_access_body(&updated, &roles, permissions);
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
            message: None,
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&response)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(response))
}

fn parse_permissions(raw_permissions: &serde_json::Value) -> Result<Vec<String>, ApiError> {
    normalize_permission_tree(raw_permissions)
}

/// Restricts a permission-editor save to the acting user's own sphere: only
/// permissions the actor holds effectively may be added to or removed from
/// the target's *direct* grants. Permissions the target already has that lie
/// outside the actor's own set (e.g. role-derived, or granted earlier by
/// someone else) are untouched by this check as long as the save doesn't
/// actually change them — this is a diff against `existing_direct`, not a
/// "whole submitted set must be a subset" check like API keys use, since a
/// human target's permissions can legitimately come from sources other than
/// the acting staffer. `SERVER_ADMIN` actors are unrestricted.
async fn validate_permission_changes_are_within_actor_scope(
    state: &AppState,
    actor: &CurrentUser,
    existing_direct: &[PermissionPath],
    requested: &[PermissionPath],
) -> Result<(), ApiError> {
    let (actor_roles, actor_permissions) = fetch_user_access(state.db.as_ref(), &actor.id).await?;

    if is_server_admin(&actor_roles) {
        return Ok(());
    }

    let existing_set: BTreeSet<&PermissionPath> = existing_direct.iter().collect();
    let requested_set: BTreeSet<&PermissionPath> = requested.iter().collect();
    let actor_set: BTreeSet<&PermissionPath> = actor_permissions.iter().collect();

    for changed in requested_set.symmetric_difference(&existing_set) {
        if !actor_set.contains(changed) {
            return Err(ApiError::Unauthorized);
        }
    }

    Ok(())
}

fn build_user_access_body(
    user: &CurrentUser,
    roles: &[String],
    permissions: Vec<PermissionPath>,
) -> UserAccessBody {
    UserAccessBody {
        id: user.id.clone(),
        cid: user.cid,
        server_admin: is_server_admin(roles),
        role_names: roles.to_vec(),
        permissions: permission_tree_from_paths(&permissions),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_permissions;

    #[test]
    fn parses_nested_permissions() {
        assert_eq!(
            parse_permissions(&json!({
                "events": {
                    "items": ["update", "read"]
                }
            }))
            .unwrap(),
            vec![
                "events.items.read".to_string(),
                "events.items.update".to_string()
            ]
        );
    }
}
