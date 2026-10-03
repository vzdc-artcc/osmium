use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
};

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath, fetch_user_access, permission_tree_from_paths},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    errors::ApiError,
    jobs::roster_sync,
    models::{
        CreateVisitorApplicationRequest, ListUsersQuery,
        ManualVatusaRefreshResponse as ManualVatusaRefreshResponseBody,
        ManualVatusaRefreshResult as ManualVatusaRefreshResultBody, PaginationMeta,
        PaginationQuery, RosterUserRow, StaffPositionHoldersResponse, StaffPositionsResponse,
        UserBasicInfo, UserDetailsResponse, UserFeedbackListResponse, UserFeedbackQuery,
        UserFullInfo, UserListItem, UserListResponse, UserPrivateInfo, VisitArtccRequest,
        VisitArtccResponse, VisitorApplicationItem,
    },
    repos::{audit as audit_repo, feedback as feedback_repo, users as user_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    params(ListUsersQuery),
    responses(
        (status = 200, description = "List users", body = UserListResponse)
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListUsersQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<UserListResponse>, ApiError> {
    // Public by policy — the roster is public information on the live site
    // today (no login required). Extended per-row fields (`full`) are gated
    // by can_view_extended_directory / self-match — every authenticated
    // controller has this via the baseline `users.directory.read` grant, not
    // just staff. Roster-hidden users are excluded from the listing entirely
    // unless the viewer holds the staff-only `users.directory_private.read`
    // — deliberately a separate, stricter check, so granting the broad
    // directory permission to every controller doesn't also bypass the
    // hidden_from_roster opt-out for everyone.
    let viewer = current_user.as_ref();
    let can_view_hidden = match viewer {
        Some(v) => can_view_private_directory(&state, v).await?,
        None => false,
    };
    let can_view_extended = match viewer {
        Some(v) => can_view_extended_directory(&state, v).await?,
        None => false,
    };
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let controllers_only = query.controllers_only.unwrap_or(false);
    let role = query
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let total =
        user_repo::count_roster_users(pool, controllers_only, can_view_hidden, role).await?;
    let rows = user_repo::list_roster_users(
        pool,
        controllers_only,
        can_view_hidden,
        role,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let items = rows
        .into_iter()
        .map(|row| {
            let basic = basic_info_from_row(&row);
            let is_self = viewer.map(|v| v.cid == row.cid).unwrap_or(false);
            let full = if can_view_extended || is_self {
                Some(private_info_from_row(&row))
            } else {
                None
            };

            UserListItem { basic, full }
        })
        .collect();

    let pagination_meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        UserListResponse {
            items,
            pagination: pagination_meta,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/{cid}",
    tag = "users",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    responses(
        (status = 200, description = "User details", body = UserDetailsResponse),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<UserDetailsResponse>, ApiError> {
    // Public by policy — see list_users.
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = user_repo::find_roster_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(ApiJson::new(
        build_user_details_response(&state, current_user.as_ref(), row).await?,
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/{cid}/staff-positions",
    tag = "users",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    responses(
        (status = 200, description = "Currently held staff position tags", body = StaffPositionsResponse)
    )
)]
pub async fn get_staff_positions(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<StaffPositionsResponse>, ApiError> {
    // Public by policy, same as get_user — these are roster/profile display
    // tags, not permissions.
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let positions = user_repo::list_held_staff_positions(pool, cid).await?;
    Ok(ApiJson::new(
        StaffPositionsResponse { cid, positions },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/staff-positions/{position}/holders",
    tag = "users",
    params(("position" = String, Path, description = "Staff position code, e.g. ATM")),
    responses(
        (status = 200, description = "Controllers holding the given staff position", body = StaffPositionHoldersResponse)
    )
)]
pub async fn get_staff_position_holders(
    State(state): State<AppState>,
    Path(position): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<StaffPositionHoldersResponse>, ApiError> {
    // Public by policy, like get_staff_positions — display tags, not permissions.
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let holders = user_repo::list_staff_position_holders(pool, &position).await?;
    Ok(ApiJson::new(
        StaffPositionHoldersResponse { position, holders },
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/visit-artcc",
    tag = "users",
    request_body = VisitArtccRequest,
    responses(
        (status = 200, description = "Visitor roster membership upserted", body = VisitArtccResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn visit_artcc(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<VisitArtccRequest>,
) -> Result<Json<VisitArtccResponse>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(viewer),
        None,
        PermissionPath::from_segments(["users", "visit_artcc"], PermissionAction::Request),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let artcc = payload.artcc.trim().to_ascii_uppercase();
    if artcc.is_empty() || artcc.len() > 8 {
        return Err(ApiError::BadRequest);
    }

    let before = user_repo::find_roster_user_by_cid(pool, viewer.cid).await?;
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    user_repo::ensure_visitor_membership(&mut tx, &viewer.id, &artcc, payload.rating.as_deref())
        .await?;

    let updated = user_repo::fetch_user_cid_artcc_rating(&mut tx, &viewer.id).await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let response = VisitArtccResponse {
        cid: updated.0,
        artcc: updated.1.unwrap_or(artcc),
        rating: updated.2,
        status: "ACTIVE".to_string(),
        roster_added: true,
    };

    let actor = audit_repo::resolve_audit_actor(pool, Some(viewer), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "VISITOR_MEMBERSHIP".to_string(),
            resource_id: Some(viewer.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(viewer.cid.to_string()),
            message: None,
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
    path = "/api/v1/users/refresh-vatusa",
    tag = "users",
    responses(
        (status = 200, description = "Current user refreshed from VATUSA", body = ManualVatusaRefreshResponseBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated"),
        (status = 503, description = "VATUSA or database unavailable")
    )
)]
pub async fn refresh_my_vatusa(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<ManualVatusaRefreshResponseBody>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(viewer),
        None,
        PermissionPath::from_segments(["users", "vatusa_refresh_self"], PermissionAction::Request),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let before = user_repo::find_roster_user_by_cid(pool, viewer.cid).await?;
    let refreshed = roster_sync::refresh_single_user_from_vatusa(pool, viewer.cid).await?;
    let response = ManualVatusaRefreshResponseBody {
        user: build_user_details_response(&state, Some(viewer), refreshed.user.clone()).await?,
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

    let actor = audit_repo::resolve_audit_actor(pool, Some(viewer), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "USER_VATUSA_REFRESH".to_string(),
            resource_id: before.as_ref().map(|row| row.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(viewer.cid.to_string()),
            message: None,
            before_state: before
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: Some(audit_repo::sanitized_snapshot(&response)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(ApiJson::new(response, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/visitor-application",
    tag = "users",
    responses(
        (status = 200, description = "Current user's visitor application, or null when absent", body = Option<VisitorApplicationItem>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn get_my_visitor_application(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Option<VisitorApplicationItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(
            ["users", "visitor_applications_self"],
            PermissionAction::Read,
        ),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let application = user_repo::find_visitor_application_by_user_id(pool, &user.id).await?;

    Ok(ApiJson::new(application, time))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/visitor-application",
    tag = "users",
    request_body = CreateVisitorApplicationRequest,
    responses(
        (status = 200, description = "Visitor application submitted", body = VisitorApplicationItem),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_visitor_application(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateVisitorApplicationRequest>,
) -> Result<ApiJson<VisitorApplicationItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(
            ["users", "visitor_applications_self"],
            PermissionAction::Request,
        ),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let home_facility = payload.home_facility.trim().to_ascii_uppercase();
    if home_facility.is_empty() || home_facility.len() > 8 {
        return Err(ApiError::BadRequest);
    }

    let why_visit = payload.why_visit.trim();
    if why_visit.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let before = user_repo::find_visitor_application_by_user_id(pool, &user.id).await?;
    let application =
        user_repo::upsert_visitor_application(pool, &user.id, &home_facility, why_visit).await?;

    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: if before.is_some() {
                "UPDATE".to_string()
            } else {
                "CREATE".to_string()
            },
            resource_type: "VISITOR_APPLICATION".to_string(),
            resource_id: Some(application.id.clone()),
            scope_type: "global".to_string(),
            scope_key: Some(user.cid.to_string()),
            message: None,
            before_state: before
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: Some(audit_repo::sanitized_snapshot(&application)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(ApiJson::new(application, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/{cid}/feedback",
    tag = "users",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        UserFeedbackQuery
    ),
    responses(
        (status = 200, description = "Feedback for a user", body = UserFeedbackListResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "User not found")
    )
)]
pub async fn get_user_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    Query(query): Query<UserFeedbackQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<UserFeedbackListResponse>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let target = user_repo::find_user_identity_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;

    if target.1 == viewer.cid {
        ensure_permission(
            &state,
            Some(viewer),
            None,
            PermissionPath::from_segments(["feedback", "items", "self"], PermissionAction::Read),
        )
        .await?;
    } else {
        ensure_permission(
            &state,
            Some(viewer),
            None,
            PermissionPath::from_segments(["users", "directory_private"], PermissionAction::Read),
        )
        .await?;
    }

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 500);
    let normalized_status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .map_or(Ok(None), |normalized| {
            if normalized != "PENDING" && normalized != "RELEASED" && normalized != "STASHED" {
                Err(ApiError::BadRequest)
            } else {
                Ok(Some(normalized))
            }
        })?;

    let total =
        feedback_repo::count_by_target(pool, &target.0, normalized_status.as_deref()).await?;

    let items = feedback_repo::list_by_target(
        pool,
        &target.0,
        normalized_status.as_deref(),
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        UserFeedbackListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

// Gates admin-only surfaces (GET /api/v1/admin/users, .../overview) and the
// hidden_from_roster bypass — staff-only, deliberately NOT part of the
// baseline every user gets, unlike `can_view_extended_directory` below.
async fn can_view_private_directory(
    state: &AppState,
    user: &CurrentUser,
) -> Result<bool, ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;

    Ok(permissions.contains(&PermissionPath::from_segments(
        ["users", "directory_private"],
        PermissionAction::Read,
    )))
}

// Gates the per-row `full` (extended profile: operating initials,
// controller_status, artcc, role_names, bio, etc.) fields on the general
// roster listing/detail endpoints. Broader than `can_view_private_directory`
// on purpose — every controller gets `users.directory.read` at login (it's
// part of the baseline self-service set), so this returns true for any
// authenticated user, not just staff. Staff (`users.directory_private.read`)
// implicitly get this too. Does NOT bypass hidden_from_roster or unlock the
// admin endpoints — those stay on `can_view_private_directory` alone.
async fn can_view_extended_directory(
    state: &AppState,
    user: &CurrentUser,
) -> Result<bool, ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;

    Ok(permissions.contains(&PermissionPath::from_segments(
        ["users", "directory"],
        PermissionAction::Read,
    )) || permissions.contains(&PermissionPath::from_segments(
        ["users", "directory_private"],
        PermissionAction::Read,
    )))
}

pub(crate) async fn build_user_details_response(
    state: &AppState,
    viewer: Option<&CurrentUser>,
    row: RosterUserRow,
) -> Result<UserDetailsResponse, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let basic = basic_info_from_row(&row);
    let is_self = viewer.map(|v| v.cid == row.cid).unwrap_or(false);

    // Public by policy — the caller's own operation-specific permission check
    // (if any) already happened before reaching this shared helper; this is
    // just response shaping (does the viewer get extended fields or not).
    // See can_view_extended_directory: every authenticated controller has
    // this via the baseline grant, not just staff.
    let can_view_extended = match viewer {
        Some(v) => can_view_extended_directory(state, v).await?,
        None => false,
    };
    if !can_view_extended && !is_self {
        return Ok(UserDetailsResponse { basic, full: None });
    }

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &row.id).await?;
    let stats = user_repo::fetch_user_stats(pool, &row.id).await?;

    Ok(UserDetailsResponse {
        basic,
        full: Some(UserFullInfo {
            profile: private_info_from_row(&row),
            roles,
            permissions: permission_tree_from_paths(&permissions),
            stats,
        }),
    })
}

fn basic_info_from_row(row: &RosterUserRow) -> UserBasicInfo {
    UserBasicInfo {
        cid: row.cid,
        name: display_name(row),
        rating: row.rating.clone(),
    }
}

fn private_info_from_row(row: &RosterUserRow) -> UserPrivateInfo {
    UserPrivateInfo {
        id: row.id.clone(),
        email: row.email.clone(),
        display_name: row.display_name.clone(),
        role: row.role.clone(),
        first_name: row.first_name.clone(),
        last_name: row.last_name.clone(),
        preferred_name: row.preferred_name.clone(),
        artcc: row.artcc.clone(),
        division: row.division.clone(),
        status: row.status.clone(),
        controller_status: row.controller_status.clone(),
        membership_status: row.membership_status.clone(),
        join_date: row.join_date,
        home_facility: row.home_facility.clone(),
        visitor_home_facility: row.visitor_home_facility.clone(),
        is_active: row.is_active,
        operating_initials: row.operating_initials.clone(),
        role_names: row.role_names.clone(),
        bio: row.bio.clone(),
        timezone: row.timezone.clone(),
        avatar_asset_id: row.avatar_asset_id.clone(),
    }
}

fn display_name(row: &RosterUserRow) -> String {
    let first = row.first_name.clone().unwrap_or_default();
    let last = row.last_name.clone().unwrap_or_default();
    let joined = format!("{} {}", first.trim(), last.trim())
        .trim()
        .to_string();

    if joined.is_empty() {
        row.display_name.clone()
    } else {
        joined
    }
}
