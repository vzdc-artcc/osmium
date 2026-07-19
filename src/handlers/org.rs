use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
        permissions::{
            AuthProfileRead, AuthProfileUpdate, SystemRead, UsersControllerStatusUpdate,
            UsersDirectoryRead,
        },
        require_permission::RequirePermission,
    },
    email::service::EmailActor,
    errors::ApiError,
    models::{
        CertificationListResponse, ControllerLifecycleCleanupSummary, ControllerLifecycleRequest,
        ControllerLifecycleResponse, CreateLoaRequest, CreateSoloCertificationRequest,
        CreateStaffingRequestRequest, CreateSuaRequest, DecideLoaRequest, JobDetailResponse,
        JobRunItem, JobRunResponse, JobStatusItem, ListLoasQuery, ListSoloCertificationsQuery,
        ListStaffingRequestsQuery, ListSuaQuery, LoaItem, LoaListResponse, PaginationMeta,
        PaginationQuery, SoloCertificationItem, SoloCertificationListResponse, StaffingRequestItem,
        StaffingRequestListResponse, SuaBlockItem, SuaListResponse, UpdateLoaRequest,
        UpdateSoloCertificationRequest,
    },
    repos::{
        audit as audit_repo,
        org::{
            certifications, controller_lifecycle, jobs as jobs_repo, loas, solo_certs,
            staffing_requests, sua_requests,
        },
        users as user_repo,
    },
    state::{AppState, JobHealth},
    time::{ApiJson, ResponseTimeContext},
};

const LOA_MIN_DAYS: i64 = 7;
const SUA_MAX_ACTIVE_REQUESTS: i64 = 2;
const SUA_MIN_DURATION_MINUTES: i64 = 30;
const SUA_MAX_DURATION_HOURS: i64 = 12;

#[derive(Debug, Serialize)]
struct JobExecutionSummary {
    processed: i64,
    details: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

#[utoipa::path(get, path = "/api/v1/loa/me", tag = "workflows", params(PaginationQuery), responses((status = 200, description = "Current user's LOAs", body = LoaListResponse), (status = 401, description = "Not authenticated")))]
pub async fn list_my_loas(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<LoaListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = loas::count_my_loas(pool, &user.id).await?;
    let rows = loas::list_my_loas(pool, &user.id, pagination.page_size, pagination.offset).await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        LoaListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/loa/me", tag = "workflows", request_body = CreateLoaRequest, responses((status = 201, description = "LOA created", body = LoaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_loa(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateLoaRequest>,
) -> Result<(StatusCode, ApiJson<LoaItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    validate_loa_range(payload.start, payload.end, payload.reason.trim())?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = loas::insert_loa(
        pool,
        &Uuid::new_v4().to_string(),
        &user.id,
        payload.start,
        payload.end,
        payload.reason.trim(),
    )
    .await?;

    record_simple_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "LOA",
        Some(row.id.clone()),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(row, time)))
}

#[utoipa::path(patch, path = "/api/v1/loa/{loa_id}", tag = "workflows", params(("loa_id" = String, Path, description = "LOA ID")), request_body = UpdateLoaRequest, responses((status = 200, description = "Updated LOA", body = LoaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "LOA not found")))]
pub async fn update_loa(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileUpdate>,
    Path(loa_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateLoaRequest>,
) -> Result<ApiJson<LoaItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    validate_loa_range(payload.start, payload.end, payload.reason.trim())?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let before = loas::fetch_loa_owned_by(pool, &loa_id, &user.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let row = loas::update_loa_row(
        pool,
        &loa_id,
        &user.id,
        payload.start,
        payload.end,
        payload.reason.trim(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;

    record_full_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "LOA",
        Some(loa_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;

    Ok(ApiJson::new(row, time))
}

#[utoipa::path(get, path = "/api/v1/admin/loa", tag = "workflows", params(PaginationQuery, ("status" = Option<String>, Query, description = "Optional LOA status filter"), ("cid" = Option<i64>, Query, description = "Optional user CID filter")), responses((status = 200, description = "LOA list", body = LoaListResponse), (status = 401, description = "Not authenticated")))]
pub async fn admin_list_loas(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersDirectoryRead>,
    Query(query): Query<ListLoasQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<LoaListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let status = query
        .status
        .as_deref()
        .map(|value| value.trim().to_ascii_uppercase());

    let total = loas::count_admin_loas(pool, status.as_deref(), query.cid).await?;
    let items = loas::list_admin_loas(
        pool,
        status.as_deref(),
        query.cid,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        LoaListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(patch, path = "/api/v1/admin/loa/{loa_id}/decision", tag = "workflows", params(("loa_id" = String, Path, description = "LOA ID")), request_body = DecideLoaRequest, responses((status = 200, description = "Updated LOA decision", body = LoaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "LOA not found")))]
pub async fn decide_loa(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    Path(loa_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<DecideLoaRequest>,
) -> Result<ApiJson<LoaItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let normalized = normalize_loa_admin_status(&payload.status)?;
    let before = loas::fetch_loa_by_id(pool, &loa_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;

    let row = loas::decide_loa_row(pool, &loa_id, normalized, actor.actor_id.as_deref())
        .await?
        .ok_or(ApiError::NotFound)?;

    maybe_send_loa_email(&state, pool, actor, &row, payload.reason.as_deref()).await;

    record_full_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "LOA",
        Some(loa_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&row)?),
    )
    .await?;

    Ok(ApiJson::new(row, time))
}

#[utoipa::path(post, path = "/api/v1/admin/loa/expire-run", tag = "workflows", responses((status = 200, description = "LOA expiration job run", body = JobRunResponse), (status = 401, description = "Not authenticated")))]
pub async fn run_loa_expiration(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<JobRunResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let run_id = jobs_repo::create_job_run(pool, "loa_expiration").await?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    let result = execute_loa_expiration(&state, pool, actor).await;
    let run = finish_job_run(pool, &run_id, result).await?;

    record_simple_audit(
        pool,
        user,
        &headers,
        "RUN",
        "JOB",
        Some(run.id.clone()),
        Some(audit_repo::sanitized_snapshot(&run)?),
    )
    .await?;

    Ok(ApiJson::new(JobRunResponse { run }, time))
}

#[utoipa::path(get, path = "/api/v1/users/{cid}/solo-certifications", tag = "workflows", params(("cid" = i64, Path, description = "User CID"), PaginationQuery), responses((status = 200, description = "User solo certifications", body = SoloCertificationListResponse), (status = 401, description = "Not authenticated")))]
pub async fn get_user_solo_certifications(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<SoloCertificationListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    // Data-dependent authorization: viewing one's own certifications only requires the
    // self-service "auth.profile.read" permission, not the admin "users.directory.read"
    // permission required to view someone else's. Not a single RequirePermission<P> case.
    if user.cid != cid {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
        )
        .await?;
    } else {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
        )
        .await?;
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let (items, total) = solo_certs::list_solo_certifications(
        pool,
        None,
        Some(cid),
        pagination.page_size,
        pagination.offset,
    )
    .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        SoloCertificationListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(get, path = "/api/v1/users/{cid}/certifications", tag = "workflows", params(("cid" = i64, Path, description = "User CID")), responses((status = 200, description = "User certifications by type", body = CertificationListResponse), (status = 401, description = "Not authenticated")))]
pub async fn get_user_certifications(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    time: ResponseTimeContext,
) -> Result<ApiJson<CertificationListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    // Same data-dependent authorization as get_user_solo_certifications above: self-view
    // needs only "auth.profile.read", viewing someone else needs "users.directory.read".
    if user.cid != cid {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
        )
        .await?;
    } else {
        ensure_permission(
            &state,
            Some(user),
            None,
            PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
        )
        .await?;
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let items = certifications::fetch_user_certifications(pool, cid).await?;

    Ok(ApiJson::new(CertificationListResponse { items }, time))
}

#[utoipa::path(get, path = "/api/v1/admin/solo-certifications", tag = "workflows", params(PaginationQuery, ("cid" = Option<i64>, Query, description = "Optional user CID filter")), responses((status = 200, description = "Solo certification list", body = SoloCertificationListResponse), (status = 401, description = "Not authenticated")))]
pub async fn admin_list_solo_certifications(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersDirectoryRead>,
    Query(query): Query<ListSoloCertificationsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<SoloCertificationListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let (items, total) = solo_certs::list_solo_certifications(
        pool,
        query.cid,
        query.cid,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        SoloCertificationListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/solo-certifications", tag = "workflows", request_body = CreateSoloCertificationRequest, responses((status = 201, description = "Solo certification created", body = SoloCertificationItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_solo_certification(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateSoloCertificationRequest>,
) -> Result<(StatusCode, ApiJson<SoloCertificationItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    if payload.position.trim().is_empty() || payload.expires <= Utc::now() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    if !solo_certs::ensure_solo_certification_type_exists(pool, &payload.certification_type_id)
        .await?
    {
        return Err(ApiError::BadRequest);
    }
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;

    let row = solo_certs::insert_solo_certification(
        pool,
        &Uuid::new_v4().to_string(),
        &payload.user_id,
        &payload.certification_type_id,
        payload.position.trim(),
        payload.expires,
        actor.actor_id.as_deref(),
    )
    .await?;

    let full = solo_certs::fetch_solo_certification(pool, &row.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    maybe_send_solo_email(&state, pool, actor, "solo.added", &full, None).await;

    record_simple_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "SOLO_CERTIFICATION",
        Some(full.id.clone()),
        Some(audit_repo::sanitized_snapshot(&full)?),
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(full, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/solo-certifications/{solo_id}", tag = "workflows", params(("solo_id" = String, Path, description = "Solo certification ID")), request_body = UpdateSoloCertificationRequest, responses((status = 200, description = "Updated solo certification", body = SoloCertificationItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "Solo certification not found")))]
pub async fn update_solo_certification(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    Path(solo_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateSoloCertificationRequest>,
) -> Result<ApiJson<SoloCertificationItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = solo_certs::fetch_solo_certification(pool, &solo_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if let Some(certification_type_id) = payload.certification_type_id.as_deref() {
        if !solo_certs::ensure_solo_certification_type_exists(pool, certification_type_id).await? {
            return Err(ApiError::BadRequest);
        }
    }
    if let Some(expires) = payload.expires {
        if expires <= Utc::now() {
            return Err(ApiError::BadRequest);
        }
    }

    let row = solo_certs::update_solo_certification_row(
        pool,
        &solo_id,
        payload.certification_type_id.as_deref(),
        payload
            .position
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        payload.expires,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

    let full = solo_certs::fetch_solo_certification(pool, &row.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    record_full_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "SOLO_CERTIFICATION",
        Some(solo_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&full)?),
    )
    .await?;

    Ok(ApiJson::new(full, time))
}

#[utoipa::path(delete, path = "/api/v1/admin/solo-certifications/{solo_id}", tag = "workflows", params(("solo_id" = String, Path, description = "Solo certification ID")), responses((status = 200, description = "Deleted solo certification", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 404, description = "Solo certification not found")))]
pub async fn delete_solo_certification(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    Path(solo_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = solo_certs::fetch_solo_certification(pool, &solo_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;

    solo_certs::delete_solo_certification_row(pool, &solo_id).await?;

    maybe_send_solo_email(&state, pool, actor, "solo.deleted", &before, None).await;

    record_full_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "SOLO_CERTIFICATION",
        Some(solo_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;

    Ok(Json(ApiMessageBody {
        message: "solo certification deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/staffing-requests/me", tag = "workflows", params(PaginationQuery), responses((status = 200, description = "Current user's staffing requests", body = StaffingRequestListResponse), (status = 401, description = "Not authenticated")))]
pub async fn list_my_staffing_requests(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<StaffingRequestListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = staffing_requests::count_my_staffing_requests(pool, &user.id).await?;
    let rows = staffing_requests::list_my_staffing_requests(
        pool,
        &user.id,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        StaffingRequestListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/staffing-requests/me", tag = "workflows", request_body = CreateStaffingRequestRequest, responses((status = 201, description = "Staffing request created", body = StaffingRequestItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_staffing_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateStaffingRequestRequest>,
) -> Result<(StatusCode, ApiJson<StaffingRequestItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let name = payload.name.trim();
    let description = payload.description.trim();
    if name.is_empty() || description.is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = staffing_requests::insert_staffing_request(
        pool,
        &Uuid::new_v4().to_string(),
        &user.id,
        name,
        description,
    )
    .await?;

    let full = staffing_requests::fetch_staffing_request(pool, &row.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    record_simple_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "STAFFING_REQUEST",
        Some(full.id.clone()),
        Some(audit_repo::sanitized_snapshot(&full)?),
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(full, time)))
}

#[utoipa::path(get, path = "/api/v1/admin/staffing-requests", tag = "workflows", params(PaginationQuery, ("cid" = Option<i64>, Query, description = "Optional user CID filter")), responses((status = 200, description = "Staffing request list", body = StaffingRequestListResponse), (status = 401, description = "Not authenticated")))]
pub async fn admin_list_staffing_requests(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersDirectoryRead>,
    Query(query): Query<ListStaffingRequestsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<StaffingRequestListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);

    let total = staffing_requests::count_admin_staffing_requests(pool, query.cid).await?;
    let items = staffing_requests::list_admin_staffing_requests(
        pool,
        query.cid,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        StaffingRequestListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(delete, path = "/api/v1/admin/staffing-requests/{request_id}", tag = "workflows", params(("request_id" = String, Path, description = "Staffing request ID")), responses((status = 200, description = "Deleted staffing request", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 404, description = "Staffing request not found")))]
pub async fn delete_staffing_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = staffing_requests::fetch_staffing_request(pool, &request_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    staffing_requests::delete_staffing_request_row(pool, &request_id).await?;

    record_full_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "STAFFING_REQUEST",
        Some(request_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;

    Ok(Json(ApiMessageBody {
        message: "staffing request deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/sua/me", tag = "workflows", params(PaginationQuery), responses((status = 200, description = "Current user's SUA requests", body = SuaListResponse), (status = 401, description = "Not authenticated")))]
pub async fn list_my_sua_requests(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    Query(query): Query<PaginationQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<SuaListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let (items, total) = sua_requests::list_sua_blocks(
        pool,
        Some(user.cid),
        pagination.page_size,
        pagination.offset,
    )
    .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        SuaListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/sua/me", tag = "workflows", request_body = CreateSuaRequest, responses((status = 201, description = "SUA request created", body = SuaBlockItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_sua_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreateSuaRequest>,
) -> Result<(StatusCode, ApiJson<SuaBlockItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    validate_sua_request(&payload)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let active_count = sua_requests::count_active_sua_for_user(pool, &user.id).await?;
    if active_count >= SUA_MAX_ACTIVE_REQUESTS {
        return Err(ApiError::BadRequest);
    }

    let mission_number = generate_mission_number(payload.start_at);
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let id = Uuid::new_v4().to_string();
    sua_requests::insert_sua_block(
        &mut *tx,
        &id,
        &user.id,
        payload.start_at,
        payload.end_at,
        payload.afiliation.trim(),
        payload.details.trim(),
        &mission_number,
    )
    .await?;

    for airspace in &payload.airspace {
        sua_requests::insert_sua_airspace(
            &mut *tx,
            &Uuid::new_v4().to_string(),
            &id,
            &airspace.identifier.trim().to_ascii_uppercase(),
            airspace.bottom_altitude.trim(),
            airspace.top_altitude.trim(),
        )
        .await?;
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    let item = sua_requests::fetch_sua_block(pool, &id)
        .await?
        .ok_or(ApiError::NotFound)?;

    record_simple_audit(
        pool,
        user,
        &headers,
        "CREATE",
        "SUA_REQUEST",
        Some(item.id.clone()),
        Some(audit_repo::sanitized_snapshot(&item)?),
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(item, time)))
}

#[utoipa::path(delete, path = "/api/v1/sua/{mission_id}", tag = "workflows", params(("mission_id" = String, Path, description = "SUA mission ID")), responses((status = 200, description = "Deleted SUA request", body = ApiMessageBody), (status = 401, description = "Not authenticated"), (status = 404, description = "SUA request not found")))]
pub async fn delete_sua_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    Path(mission_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = sua_requests::fetch_sua_block(pool, &mission_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if before.user_id != user.id {
        return Err(ApiError::Unauthorized);
    }

    sua_requests::delete_sua_block_owned(pool, &mission_id, &user.id).await?;

    record_full_audit(
        pool,
        user,
        &headers,
        "DELETE",
        "SUA_REQUEST",
        Some(mission_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        None,
    )
    .await?;

    Ok(Json(ApiMessageBody {
        message: "sua request deleted".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/sua", tag = "workflows", params(PaginationQuery, ("cid" = Option<i64>, Query, description = "Optional user CID filter")), responses((status = 200, description = "SUA request list", body = SuaListResponse), (status = 401, description = "Not authenticated")))]
pub async fn admin_list_sua_requests(
    State(state): State<AppState>,
    _permission: RequirePermission<UsersDirectoryRead>,
    Query(query): Query<ListSuaQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<SuaListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let (items, total) =
        sua_requests::list_sua_blocks(pool, query.cid, pagination.page_size, pagination.offset)
            .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        SuaListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(patch, path = "/api/v1/admin/users/{cid}/controller-lifecycle", tag = "workflows", params(("cid" = i64, Path, description = "User CID")), request_body = ControllerLifecycleRequest, responses((status = 200, description = "Updated controller lifecycle", body = ControllerLifecycleResponse), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "User not found")))]
pub async fn update_controller_lifecycle(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<ControllerLifecycleRequest>,
) -> Result<ApiJson<ControllerLifecycleResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let normalized_status = normalize_controller_status(&payload.controller_status)?;
    let before = controller_lifecycle::fetch_membership_lifecycle_row(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let cleanup_on_none = payload.cleanup_on_none.unwrap_or(true);
    let mut cleanup = ControllerLifecycleCleanupSummary {
        training_assignment_requests_deleted: 0,
        training_assignments_deleted: 0,
        loas_deleted: 0,
        operating_initials_assigned: false,
        operating_initials_cleared: false,
        welcome_message_enabled: false,
    };

    let artcc = payload
        .artcc
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase);

    controller_lifecycle::update_membership_status(
        &mut *tx,
        &before.user_id,
        normalized_status,
        artcc.as_deref(),
    )
    .await?;

    if normalized_status == "NONE" {
        cleanup.operating_initials_cleared =
            controller_lifecycle::clear_operating_initials(&mut *tx, &before.user_id).await?;

        if cleanup_on_none {
            cleanup.training_assignment_requests_deleted =
                controller_lifecycle::delete_training_assignment_requests_for_user(
                    &mut *tx,
                    &before.user_id,
                )
                .await?;
            cleanup.training_assignments_deleted =
                controller_lifecycle::delete_training_assignments_for_user(
                    &mut *tx,
                    &before.user_id,
                )
                .await?;
            cleanup.loas_deleted = loas::delete_loas_for_user(&mut *tx, &before.user_id).await?;
        }
    } else {
        if before.operating_initials.is_none() {
            cleanup.operating_initials_assigned = user_repo::ensure_operating_initials(
                &mut tx,
                &before.user_id,
                before.first_name.as_deref(),
                before.last_name.as_deref(),
                &before.display_name,
            )
            .await?
            .is_some();
        }

        if before.controller_status == "NONE" && !before.show_welcome_message {
            cleanup.welcome_message_enabled =
                controller_lifecycle::enable_welcome_message(&mut *tx, &before.user_id).await?;
        }
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    let after = controller_lifecycle::fetch_membership_lifecycle_row(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    let response = ControllerLifecycleResponse {
        cid: after.cid,
        controller_status: after.controller_status,
        artcc: Some(after.artcc),
        cleanup,
    };

    record_full_audit(
        pool,
        user,
        &headers,
        "UPDATE",
        "USER_CONTROLLER_LIFECYCLE",
        Some(after.user_id),
        Some(audit_repo::sanitized_snapshot(&before)?),
        Some(audit_repo::sanitized_snapshot(&response)?),
    )
    .await?;

    Ok(ApiJson::new(response, time))
}

#[utoipa::path(get, path = "/api/v1/admin/jobs", tag = "workflows", responses((status = 200, description = "Backend jobs", body = [JobStatusItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_jobs(
    State(state): State<AppState>,
    _permission: RequirePermission<SystemRead>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Vec<JobStatusItem>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let health = state
        .job_health
        .read()
        .map_err(|_| ApiError::Internal)?
        .clone();
    let mut items = Vec::new();
    for job_name in [
        "stats_sync",
        "roster_sync",
        "loa_expiration",
        "solo_expiration",
        "event_automation",
    ] {
        items.push(build_job_status(pool, &health, job_name).await?);
    }
    Ok(ApiJson::new(items, time))
}

#[utoipa::path(get, path = "/api/v1/admin/jobs/{job_name}", tag = "workflows", params(("job_name" = String, Path, description = "Job name")), responses((status = 200, description = "Backend job detail", body = JobDetailResponse), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn get_job(
    State(state): State<AppState>,
    _permission: RequirePermission<SystemRead>,
    Path(job_name): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<JobDetailResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let health = state
        .job_health
        .read()
        .map_err(|_| ApiError::Internal)?
        .clone();
    let status = build_job_status(pool, &health, &job_name).await?;
    let recent_runs = jobs_repo::list_recent_job_runs(pool, &job_name).await?;
    Ok(ApiJson::new(
        JobDetailResponse {
            status,
            recent_runs,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/jobs/{job_name}/run", tag = "workflows", params(("job_name" = String, Path, description = "Job name")), responses((status = 200, description = "Triggered backend job", body = JobRunResponse), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn run_job(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<UsersControllerStatusUpdate>,
    Path(job_name): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<JobRunResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let run_id = jobs_repo::create_job_run(pool, &job_name).await?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    let result = match job_name.as_str() {
        "loa_expiration" => execute_loa_expiration(&state, pool, actor).await,
        "solo_expiration" => execute_solo_expiration(&state, pool, actor).await,
        "event_automation" => execute_event_automation(pool).await,
        _ => return Err(ApiError::BadRequest),
    };
    let run = finish_job_run(pool, &run_id, result).await?;

    record_simple_audit(
        pool,
        user,
        &headers,
        "RUN",
        "JOB",
        Some(run.id.clone()),
        Some(audit_repo::sanitized_snapshot(&run)?),
    )
    .await?;

    Ok(ApiJson::new(JobRunResponse { run }, time))
}

async fn build_job_status(
    pool: &sqlx::PgPool,
    health: &JobHealth,
    job_name: &str,
) -> Result<JobStatusItem, ApiError> {
    let latest_run = jobs_repo::fetch_latest_job_run(pool, job_name).await?;

    let item = match job_name {
        "stats_sync" => JobStatusItem {
            job_name: job_name.to_string(),
            enabled: health.stats_sync.enabled,
            last_started_at: health.stats_sync.live.last_started_at,
            last_finished_at: health.stats_sync.live.last_finished_at,
            last_success_at: health.stats_sync.live.last_success_at,
            last_result_ok: health.stats_sync.live.last_result_ok,
            last_error: health.stats_sync.live.last_error.clone(),
            latest_run,
        },
        "roster_sync" => JobStatusItem {
            job_name: job_name.to_string(),
            enabled: health.roster_sync.enabled,
            last_started_at: health.roster_sync.last_started_at,
            last_finished_at: health.roster_sync.last_finished_at,
            last_success_at: health.roster_sync.last_success_at,
            last_result_ok: health.roster_sync.last_result_ok,
            last_error: health.roster_sync.last_error.clone(),
            latest_run,
        },
        _ => {
            let enabled = true;
            let last_started_at = latest_run.as_ref().map(|row| row.started_at);
            let last_finished_at = latest_run.as_ref().and_then(|row| row.finished_at);
            let last_success_at = latest_run
                .as_ref()
                .filter(|row| row.status == "succeeded")
                .and_then(|row| row.finished_at);
            let last_result_ok = latest_run.as_ref().map(|row| row.status == "succeeded");
            let last_error = latest_run.as_ref().and_then(|row| row.error_text.clone());

            JobStatusItem {
                job_name: job_name.to_string(),
                enabled,
                last_started_at,
                last_finished_at,
                last_success_at,
                last_result_ok,
                last_error,
                latest_run,
            }
        }
    };

    Ok(item)
}

async fn execute_loa_expiration(
    state: &AppState,
    pool: &sqlx::PgPool,
    actor: audit_repo::AuditActor,
) -> Result<JobExecutionSummary, ApiError> {
    let items = loas::list_expired_approved_loas(pool).await?;

    let mut processed = 0;
    for item in &items {
        loas::expire_loa_row(pool, &item.id).await?;
        maybe_send_loa_email(state, pool, actor.clone(), item, None).await;
        processed += 1;
    }

    Ok(JobExecutionSummary {
        processed,
        details: json!({ "expired_loa_ids": items.into_iter().map(|item| item.id).collect::<Vec<_>>() }),
    })
}

async fn execute_solo_expiration(
    state: &AppState,
    pool: &sqlx::PgPool,
    actor: audit_repo::AuditActor,
) -> Result<JobExecutionSummary, ApiError> {
    let items = solo_certs::list_expired_solo_certifications(pool).await?;

    let mut processed = 0;
    for item in &items {
        solo_certs::delete_solo_certification_row(pool, &item.id).await?;
        maybe_send_solo_email(state, pool, actor.clone(), "solo.expired", item, None).await;
        processed += 1;
    }

    Ok(JobExecutionSummary {
        processed,
        details: json!({ "expired_solo_ids": items.into_iter().map(|item| item.id).collect::<Vec<_>>() }),
    })
}

async fn execute_event_automation(pool: &sqlx::PgPool) -> Result<JobExecutionSummary, ApiError> {
    let lock_threshold = Utc::now() + Duration::hours(24);
    let locked = jobs_repo::lock_events_near_start(pool, lock_threshold).await?;
    let archived = jobs_repo::archive_ended_events(pool, Utc::now() - Duration::hours(24)).await?;

    Ok(JobExecutionSummary {
        processed: locked + archived,
        details: json!({
            "positions_locked": locked,
            "events_archived": archived
        }),
    })
}

async fn maybe_send_loa_email(
    state: &AppState,
    pool: &sqlx::PgPool,
    actor: audit_repo::AuditActor,
    loa: &LoaItem,
    decision_reason: Option<&str>,
) {
    let template_id = match loa.status.as_str() {
        "APPROVED" => "loa.approved",
        "DENIED" => "loa.denied",
        "INACTIVE" if loa.end < Utc::now() => "loa.expired",
        "INACTIVE" => "loa.deleted",
        _ => return,
    };

    let payload = match template_id {
        "loa.approved" => json!({
            "controller_name": loa.display_name.clone().unwrap_or_else(|| "Controller".to_string()),
            "loa_start": loa.start.format("%Y-%m-%d").to_string(),
            "loa_end": loa.end.format("%Y-%m-%d").to_string()
        }),
        "loa.denied" => json!({
            "controller_name": loa.display_name.clone().unwrap_or_else(|| "Controller".to_string()),
            "reason": decision_reason
        }),
        "loa.deleted" => json!({
            "controller_name": loa.display_name.clone().unwrap_or_else(|| "Controller".to_string()),
            "reason": decision_reason
        }),
        _ => json!({
            "controller_name": loa.display_name.clone().unwrap_or_else(|| "Controller".to_string())
        }),
    };

    let email_actor = EmailActor {
        actor_id: actor.actor_id,
        user_id: None,
        service_account_id: None,
        request_source: "api".to_string(),
    };
    let _ = state
        .email
        .enqueue_to_users(
            pool,
            email_actor,
            template_id.to_string(),
            payload,
            vec![loa.user_id.clone()],
        )
        .await;
}

async fn maybe_send_solo_email(
    state: &AppState,
    pool: &sqlx::PgPool,
    actor: audit_repo::AuditActor,
    template_id: &str,
    solo: &SoloCertificationItem,
    reason: Option<&str>,
) {
    let payload = match template_id {
        "solo.added" => json!({
            "controller_name": solo.display_name.clone().unwrap_or_else(|| "Controller".to_string()),
            "position": solo.position,
            "expires": solo.expires.format("%Y-%m-%d").to_string()
        }),
        "solo.deleted" => json!({
            "controller_name": solo.display_name.clone().unwrap_or_else(|| "Controller".to_string()),
            "position": solo.position,
            "reason": reason
        }),
        _ => json!({
            "controller_name": solo.display_name.clone().unwrap_or_else(|| "Controller".to_string()),
            "position": solo.position
        }),
    };

    let email_actor = EmailActor {
        actor_id: actor.actor_id,
        user_id: None,
        service_account_id: None,
        request_source: "api".to_string(),
    };
    let _ = state
        .email
        .enqueue_to_users(
            pool,
            email_actor,
            template_id.to_string(),
            payload,
            vec![solo.user_id.clone()],
        )
        .await;
}

async fn finish_job_run(
    pool: &sqlx::PgPool,
    run_id: &str,
    result: Result<JobExecutionSummary, ApiError>,
) -> Result<JobRunItem, ApiError> {
    match result {
        Ok(summary) => {
            jobs_repo::finish_job_run_success(
                pool,
                run_id,
                json!({
                    "processed": summary.processed,
                    "details": summary.details
                }),
            )
            .await?;
        }
        Err(err) => {
            jobs_repo::finish_job_run_failure(pool, run_id, &err.to_string()).await?;
        }
    }

    jobs_repo::fetch_job_run(pool, run_id).await
}

fn validate_loa_range(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    reason: &str,
) -> Result<(), ApiError> {
    if reason.is_empty() || end <= start || end - start < Duration::days(LOA_MIN_DAYS) {
        return Err(ApiError::BadRequest);
    }
    Ok(())
}

fn normalize_loa_admin_status(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "APPROVED" => Ok("APPROVED"),
        "DENIED" => Ok("DENIED"),
        "INACTIVE" => Ok("INACTIVE"),
        _ => Err(ApiError::BadRequest),
    }
}

fn normalize_controller_status(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "HOME" => Ok("HOME"),
        "VISITOR" => Ok("VISITOR"),
        "NONE" => Ok("NONE"),
        _ => Err(ApiError::BadRequest),
    }
}

fn validate_sua_request(payload: &CreateSuaRequest) -> Result<(), ApiError> {
    if payload.afiliation.trim().is_empty()
        || payload.details.trim().is_empty()
        || payload.start_at <= Utc::now()
        || payload.end_at <= payload.start_at
    {
        return Err(ApiError::BadRequest);
    }
    let duration = payload.end_at - payload.start_at;
    if duration < Duration::minutes(SUA_MIN_DURATION_MINUTES)
        || duration > Duration::hours(SUA_MAX_DURATION_HOURS)
        || payload.airspace.is_empty()
    {
        return Err(ApiError::BadRequest);
    }

    for airspace in &payload.airspace {
        if airspace.identifier.trim().is_empty()
            || !is_valid_flight_level(&airspace.bottom_altitude)
            || !is_valid_flight_level(&airspace.top_altitude)
        {
            return Err(ApiError::BadRequest);
        }
    }

    Ok(())
}

fn is_valid_flight_level(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() == 3 && trimmed.chars().all(|ch| ch.is_ascii_digit())
}

fn generate_mission_number(start_at: DateTime<Utc>) -> String {
    let seed = Utc::now().timestamp_subsec_nanos();
    if start_at <= Utc::now() + Duration::hours(24) {
        let mut rng = seed;
        let digits = 100 + (rng % 900);
        rng /= 900;
        let suffix = ((rng % 26) as u8 + b'A') as char;
        format!("{digits}{suffix}")
    } else {
        let digits = 1000 + (seed % 9000);
        digits.to_string()
    }
}

async fn record_simple_audit(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    headers: &HeaderMap,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    after_state: Option<Value>,
) -> Result<(), ApiError> {
    record_full_audit(
        pool,
        user,
        headers,
        action,
        resource_type,
        resource_id,
        None,
        after_state,
    )
    .await
}

async fn record_full_audit(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    headers: &HeaderMap,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    before_state: Option<Value>,
    after_state: Option<Value>,
) -> Result<(), ApiError> {
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            scope_type: "global".to_string(),
            scope_key: Some(user.cid.to_string()),
            before_state,
            after_state,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}
