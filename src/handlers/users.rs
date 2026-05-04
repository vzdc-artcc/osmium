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
    models::{
        CreateVisitorApplicationRequest, FeedbackItem, ListUsersQuery, RosterUserRow,
        UserBasicInfo, UserDetailsResponse, UserFeedbackQuery, UserFullInfo, UserListItem,
        UserPrivateInfo, VisitArtccRequest, VisitArtccResponse, VisitorApplicationItem,
    },
    repos::{audit as audit_repo, users as user_repo},
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/api/v1/user",
    tag = "users",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of users to return"),
        ("offset" = Option<i64>, Query, description = "Pagination offset")
    ),
    responses(
        (status = 200, description = "List users", body = [UserListItem]),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<Vec<UserListItem>>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(viewer),
        None,
        PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let can_view_private = can_view_private_directory(&state, viewer).await?;
    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let rows = user_repo::list_roster_users(pool, limit, offset).await?;

    let items = rows
        .into_iter()
        .map(|row| {
            let basic = basic_info_from_row(&row);
            let full = if can_view_private || row.cid == viewer.cid {
                Some(private_info_from_row(&row))
            } else {
                None
            };

            UserListItem { basic, full }
        })
        .collect();

    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/api/v1/user/{cid}",
    tag = "users",
    params(
        ("cid" = i64, Path, description = "VATSIM CID")
    ),
    responses(
        (status = 200, description = "User details", body = UserDetailsResponse),
        (status = 400, description = "Invalid CID"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn get_user(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
) -> Result<Json<UserDetailsResponse>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = user_repo::find_roster_user_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let basic = basic_info_from_row(&row);
    let is_self = row.cid == viewer.cid;
    if is_self {
        ensure_permission(
            &state,
            Some(viewer),
            None,
            PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
        )
        .await?;
    } else {
        ensure_permission(
            &state,
            Some(viewer),
            None,
            PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
        )
        .await?;
    }
    let can_view_private = can_view_private_directory(&state, viewer).await?;
    let can_view_full = can_view_private || is_self;

    if !can_view_full {
        return Ok(Json(UserDetailsResponse { basic, full: None }));
    }

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &row.id).await?;
    let stats = user_repo::fetch_user_stats(pool, &row.id).await?;

    Ok(Json(UserDetailsResponse {
        basic,
        full: Some(UserFullInfo {
            profile: private_info_from_row(&row),
            roles,
            permissions: permission_tree_from_paths(&permissions),
            stats,
        }),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/user/visit-artcc",
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
    path = "/api/v1/user/visitor-application",
    tag = "users",
    responses(
        (status = 200, description = "Current user's visitor application, or null when absent", body = Option<VisitorApplicationItem>),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn get_my_visitor_application(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Option<VisitorApplicationItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(
            ["users", "visitor_applications", "self"],
            PermissionAction::Read,
        ),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let application = user_repo::find_visitor_application_by_user_id(pool, &user.id).await?;

    Ok(Json(application))
}

#[utoipa::path(
    post,
    path = "/api/v1/user/visitor-application",
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
    Json(payload): Json<CreateVisitorApplicationRequest>,
) -> Result<Json<VisitorApplicationItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(
            ["users", "visitor_applications", "self"],
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
            before_state: before
                .as_ref()
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: Some(audit_repo::sanitized_snapshot(&application)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(Json(application))
}

#[utoipa::path(
    get,
    path = "/api/v1/user/{cid}/feedback",
    tag = "users",
    params(
        ("cid" = i64, Path, description = "VATSIM CID"),
        ("limit" = Option<i64>, Query, description = "Maximum rows"),
        ("offset" = Option<i64>, Query, description = "Pagination offset"),
        ("status" = Option<String>, Query, description = "Optional feedback status filter")
    ),
    responses(
        (status = 200, description = "Feedback for a user", body = [FeedbackItem]),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn get_user_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    Query(query): Query<UserFeedbackQuery>,
) -> Result<Json<Vec<FeedbackItem>>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let target = user_repo::find_user_identity_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::BadRequest)?;

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

    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
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

    let items = sqlx::query_as::<_, FeedbackItem>(
        r#"
        select
            id,
            submitter_user_id,
            target_user_id,
            pilot_callsign,
            controller_position,
            rating,
            comments,
            staff_comments,
            status,
            submitted_at,
            decided_at,
            decided_by
        from feedback.feedback_items
        where target_user_id = $1
          and ($2::text is null or status = $2)
        order by submitted_at desc
        limit $3 offset $4
        "#,
    )
    .bind(&target.0)
    .bind(normalized_status.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(items))
}

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
        artcc: row.artcc.clone(),
        division: row.division.clone(),
        status: row.status.clone(),
        controller_status: row.controller_status.clone(),
        membership_status: row.membership_status.clone(),
        join_date: row.join_date,
        home_facility: row.home_facility.clone(),
        visitor_home_facility: row.visitor_home_facility.clone(),
        is_active: row.is_active,
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
