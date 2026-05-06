use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
};

use crate::{
    auth::{
        acl::{
            PermissionAction, PermissionPath, fetch_access_catalog, fetch_user_access,
            is_server_admin, normalize_permission_tree, permission_tree_from_names,
            permission_tree_from_paths,
        },
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
    },
    errors::ApiError,
    jobs::roster_sync,
    models::{
        AccessCatalogBody, AclDebugBody, AdminUserListResponse, AuditLogListResponse,
        DecideVisitorApplicationRequest, ListAuditLogsQuery, ListUsersQuery,
        ListVisitorApplicationsQuery,
        ManualVatusaRefreshResponse as ManualVatusaRefreshResponseBody,
        ManualVatusaRefreshResult as ManualVatusaRefreshResultBody, PaginationMeta,
        PaginationQuery, SetControllerStatusBody, SetControllerStatusRequest,
        UpdateUserAccessRequest, UserAccessBody, UserOverviewBody, VisitorApplicationItem,
        VisitorApplicationListResponse,
    },
    repos::{access as access_repo, audit as audit_repo, users as user_repo},
    state::AppState,
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
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<AclDebugBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["access", "self"], PermissionAction::Read),
    )
    .await?;

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
        PermissionPath::from_segments(["access", "users"], PermissionAction::Read),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;

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
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<AccessCatalogBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["access", "catalog"], PermissionAction::Read),
    )
    .await?;

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
    params(
        PaginationQuery,
        ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
        ("resource_id" = Option<String>, Query, description = "Filter by resource id"),
        ("actor_id" = Option<String>, Query, description = "Filter by actor id"),
        ("actor_type" = Option<String>, Query, description = "Filter by actor type"),
        ("scope_type" = Option<String>, Query, description = "Filter by scope type"),
        ("scope_key" = Option<String>, Query, description = "Filter by scope key"),
        ("action" = Option<String>, Query, description = "Filter by action")
    ),
    responses(
        (status = 200, description = "Audit log rows", body = AuditLogListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_audit_logs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Query(query): Query<ListAuditLogsQuery>,
) -> Result<Json<AuditLogListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["audit", "logs"], PermissionAction::Read),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 250);
    let normalized_action = query
        .action
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase());
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from access.audit_logs l
        left join access.actors a on a.id = l.actor_id
        where ($1::text is null or l.resource_type = $1)
          and ($2::text is null or l.resource_id = $2)
          and ($3::text is null or l.actor_id = $3)
          and ($4::text is null or a.actor_type = $4)
          and ($5::text is null or l.scope_type = $5)
          and ($6::text is null or l.scope_key = $6)
          and ($7::text is null or l.action = $7)
        "#,
    )
    .bind(query.resource_type.as_deref())
    .bind(query.resource_id.as_deref())
    .bind(query.actor_id.as_deref())
    .bind(query.actor_type.as_deref())
    .bind(query.scope_type.as_deref())
    .bind(query.scope_key.as_deref())
    .bind(normalized_action.as_deref())
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let rows = audit_repo::fetch_audit_logs(
        pool,
        &audit_repo::AuditLogFilters {
            resource_type: query.resource_type,
            resource_id: query.resource_id,
            actor_id: query.actor_id,
            actor_type: query.actor_type,
            scope_type: query.scope_type,
            scope_key: query.scope_key,
            action: normalized_action,
            limit: pagination.page_size,
            offset: pagination.offset,
        },
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(Json(AuditLogListResponse {
        items: rows,
        total: meta.total,
        page: meta.page,
        page_size: meta.page_size,
        total_pages: meta.total_pages,
        has_next: meta.has_next,
        has_prev: meta.has_prev,
    }))
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
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
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
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
) -> Result<Json<ManualVatusaRefreshResponseBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["users", "vatusa_refresh"], PermissionAction::Request),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = user_repo::find_roster_user_by_cid(pool, cid).await?;
    let refreshed = roster_sync::refresh_single_user_from_vatusa(pool, cid).await?;
    let response = ManualVatusaRefreshResponseBody {
        user: crate::handlers::users::build_user_details_response(
            &state,
            user,
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

#[utoipa::path(
    get,
    path = "/api/v1/admin/visitor-applications",
    tag = "admin",
    params(
        PaginationQuery,
        ("status" = Option<String>, Query, description = "Filter by application status")
    ),
    responses(
        (status = 200, description = "Visitor applications", body = VisitorApplicationListResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_visitor_applications(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Query(query): Query<ListVisitorApplicationsQuery>,
) -> Result<Json<VisitorApplicationListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["users", "visitor_applications"], PermissionAction::Read),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let normalized_status = normalize_visitor_application_status_filter(query.status.as_deref())?;
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from training.visitor_applications
        where ($1::text is null or status = $1)
        "#,
    )
    .bind(normalized_status.as_deref())
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    let items =
        user_repo::list_visitor_applications(
            pool,
            normalized_status.as_deref(),
            pagination.page_size,
            pagination.offset,
        )
        .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(Json(VisitorApplicationListResponse {
        items,
        total: meta.total,
        page: meta.page,
        page_size: meta.page_size,
        total_pages: meta.total_pages,
        has_next: meta.has_next,
        has_prev: meta.has_prev,
    }))
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
        (status = 401, description = "Not authorized")
    )
)]
pub async fn decide_visitor_application(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(application_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<DecideVisitorApplicationRequest>,
) -> Result<Json<VisitorApplicationItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["users", "visitor_applications"], PermissionAction::Decide),
    )
    .await?;

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
        .ok_or(ApiError::BadRequest)?;

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
    .ok_or(ApiError::BadRequest)?;

    audit_repo::record_audit(
        &mut *tx,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "VISITOR_APPLICATION".to_string(),
            resource_id: Some(after.id.clone()),
            scope_type: "global".to_string(),
            scope_key: after.cid.map(|cid| cid.to_string()),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(after))
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<AdminUserListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["users", "directory_private"], PermissionAction::Read),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = sqlx::query_scalar::<_, i64>("select count(*)::bigint from org.v_user_roster_profile")
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    let users = user_repo::list_admin_users(pool, pagination.page_size, pagination.offset).await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(Json(AdminUserListResponse {
        items: users,
        total: meta.total,
        page: meta.page,
        page_size: meta.page_size,
        total_pages: meta.total_pages,
        has_next: meta.has_next,
        has_prev: meta.has_prev,
    }))
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
        PermissionPath::from_segments(["users", "directory_private"], PermissionAction::Read),
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
        permissions: permission_tree_from_paths(&permissions),
        stats,
    }))
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
        PermissionPath::from_segments(["access", "users"], PermissionAction::Update),
    )
    .await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let parsed_permissions = parse_permissions(&payload.permissions)?;

    let target_user_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let target_before = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let (before_roles, before_permissions) =
        fetch_user_access(state.db.as_ref(), &target_before.id).await?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    access_repo::replace_user_permissions(&mut tx, &target_user_id, &parsed_permissions).await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let updated = access_repo::find_current_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;
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
            before_state: Some(audit_repo::sanitize_value(serde_json::json!({
                "user": target_before,
                "server_admin": is_server_admin(&before_roles),
                "permissions": permission_tree_from_paths(&before_permissions),
            }))),
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

fn build_user_access_body(
    user: &CurrentUser,
    roles: &[String],
    permissions: Vec<PermissionPath>,
) -> UserAccessBody {
    UserAccessBody {
        id: user.id.clone(),
        cid: user.cid,
        server_admin: is_server_admin(roles),
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
