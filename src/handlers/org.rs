use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    email::service::EmailActor,
    errors::ApiError,
    models::{PaginationMeta, PaginationQuery},
    repos::{audit as audit_repo, users as user_repo},
    state::{AppState, JobHealth},
};

const LOA_MIN_DAYS: i64 = 7;
const SUA_MAX_ACTIVE_REQUESTS: i64 = 2;
const SUA_MIN_DURATION_MINUTES: i64 = 30;
const SUA_MAX_DURATION_HOURS: i64 = 12;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct LoaItem {
    pub id: String,
    pub user_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub reason: String,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by_actor_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLoaRequest {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateLoaRequest {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideLoaRequest {
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListLoasQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoaListResponse {
    pub items: Vec<LoaItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SoloCertificationItem {
    pub id: String,
    pub user_id: String,
    pub certification_type_id: String,
    pub position: String,
    pub expires: DateTime<Utc>,
    pub granted_at: DateTime<Utc>,
    pub granted_by_actor_id: Option<String>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
    pub certification_type_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSoloCertificationRequest {
    pub user_id: String,
    pub certification_type_id: String,
    pub position: String,
    pub expires: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSoloCertificationRequest {
    pub certification_type_id: Option<String>,
    pub position: Option<String>,
    pub expires: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListSoloCertificationsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SoloCertificationListResponse {
    pub items: Vec<SoloCertificationItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct StaffingRequestItem {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateStaffingRequestRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListStaffingRequestsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffingRequestListResponse {
    pub items: Vec<StaffingRequestItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SuaAirspaceItem {
    pub id: String,
    pub sua_block_id: String,
    pub identifier: String,
    pub bottom_altitude: String,
    pub top_altitude: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SuaBlockItem {
    pub id: String,
    pub user_id: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub afiliation: String,
    pub details: String,
    pub mission_number: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
    pub airspace: Vec<SuaAirspaceItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSuaAirspaceRequest {
    pub identifier: String,
    pub bottom_altitude: String,
    pub top_altitude: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSuaRequest {
    pub afiliation: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub details: String,
    pub airspace: Vec<CreateSuaAirspaceRequest>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListSuaQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SuaListResponse {
    pub items: Vec<SuaBlockItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ControllerLifecycleRequest {
    pub controller_status: String,
    pub artcc: Option<String>,
    pub cleanup_on_none: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ControllerLifecycleCleanupSummary {
    pub training_assignment_requests_deleted: i64,
    pub training_assignments_deleted: i64,
    pub loas_deleted: i64,
    pub operating_initials_assigned: bool,
    pub operating_initials_cleared: bool,
    pub welcome_message_enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ControllerLifecycleResponse {
    pub cid: i64,
    pub controller_status: String,
    pub artcc: Option<String>,
    pub cleanup: ControllerLifecycleCleanupSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct JobRunItem {
    pub id: String,
    pub job_name: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub result_summary: Option<Value>,
    pub error_text: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobStatusItem {
    pub job_name: String,
    pub enabled: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_result_ok: Option<bool>,
    pub last_error: Option<String>,
    pub latest_run: Option<JobRunItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobDetailResponse {
    pub status: JobStatusItem,
    pub recent_runs: Vec<JobRunItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JobRunResponse {
    pub run: JobRunItem,
}

#[derive(Debug, sqlx::FromRow)]
struct SuaBlockRow {
    id: String,
    user_id: String,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    afiliation: String,
    details: String,
    mission_number: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    cid: Option<i64>,
    display_name: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct MembershipLifecycleRow {
    user_id: String,
    cid: i64,
    controller_status: String,
    artcc: String,
    operating_initials: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    display_name: String,
    show_welcome_message: bool,
}

#[derive(Debug, Serialize)]
struct JobExecutionSummary {
    processed: i64,
    details: Value,
}

#[utoipa::path(get, path = "/api/v1/loa/me", tag = "workflows", params(PaginationQuery), responses((status = 200, description = "Current user's LOAs", body = LoaListResponse), (status = 401, description = "Not authenticated")))]
pub async fn list_my_loas(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<LoaListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = sqlx::query_scalar::<_, i64>("select count(*)::bigint from org.loas where user_id = $1")
        .bind(&user.id)
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    let rows = sqlx::query_as::<_, LoaItem>(
        r#"
        select id, user_id, start, "end", reason, status, submitted_at, decided_at, decided_by_actor_id, created_at, updated_at, null::bigint as cid, null::text as display_name
        from org.loas
        where user_id = $1
        order by start desc, created_at desc, id asc
        limit $2 offset $3
        "#,
    )
    .bind(&user.id)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(LoaListResponse { items: rows, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(post, path = "/api/v1/loa/me", tag = "workflows", request_body = CreateLoaRequest, responses((status = 201, description = "LOA created", body = LoaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_loa(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateLoaRequest>,
) -> Result<(StatusCode, Json<LoaItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Update),
    )
    .await?;
    validate_loa_range(payload.start, payload.end, payload.reason.trim())?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = sqlx::query_as::<_, LoaItem>(
        r#"
        insert into org.loas (id, user_id, start, "end", reason, status, submitted_at, created_at, updated_at)
        values ($1, $2, $3, $4, $5, 'PENDING', now(), now(), now())
        returning
            id,
            user_id,
            start,
            "end",
            reason,
            status,
            submitted_at,
            decided_at,
            decided_by_actor_id,
            created_at,
            updated_at,
            null::bigint as cid,
            null::text as display_name
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&user.id)
    .bind(payload.start)
    .bind(payload.end)
    .bind(payload.reason.trim())
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)?;

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

    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(patch, path = "/api/v1/loa/{loa_id}", tag = "workflows", params(("loa_id" = String, Path, description = "LOA ID")), request_body = UpdateLoaRequest, responses((status = 200, description = "Updated LOA", body = LoaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_loa(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(loa_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateLoaRequest>,
) -> Result<Json<LoaItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Update),
    )
    .await?;
    validate_loa_range(payload.start, payload.end, payload.reason.trim())?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let before = fetch_loa_owned_by(pool, &loa_id, &user.id).await?;
    let row = sqlx::query_as::<_, LoaItem>(
        r#"
        update org.loas
        set start = $3,
            "end" = $4,
            reason = $5,
            status = 'PENDING',
            decided_at = null,
            decided_by_actor_id = null,
            updated_at = now()
        where id = $1
          and user_id = $2
          and status = 'PENDING'
        returning
            id,
            user_id,
            start,
            "end",
            reason,
            status,
            submitted_at,
            decided_at,
            decided_by_actor_id,
            created_at,
            updated_at,
            null::bigint as cid,
            null::text as display_name
        "#,
    )
    .bind(&loa_id)
    .bind(&user.id)
    .bind(payload.start)
    .bind(payload.end)
    .bind(payload.reason.trim())
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

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

    Ok(Json(row))
}

#[utoipa::path(get, path = "/api/v1/admin/loa", tag = "workflows", params(PaginationQuery, ("status" = Option<String>, Query, description = "Optional LOA status filter"), ("cid" = Option<i64>, Query, description = "Optional user CID filter")), responses((status = 200, description = "LOA list", body = LoaListResponse), (status = 401, description = "Not authenticated")))]
pub async fn admin_list_loas(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListLoasQuery>,
) -> Result<Json<LoaListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let status = query
        .status
        .as_deref()
        .map(|value| value.trim().to_ascii_uppercase());

    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.loas l
        join identity.users u on u.id = l.user_id
        where ($1::text is null or l.status = $1)
          and ($2::bigint is null or u.cid = $2)
        "#,
    )
    .bind(status.as_deref())
    .bind(query.cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let items = sqlx::query_as::<_, LoaItem>(
        r#"
        select
            l.id,
            l.user_id,
            l.start,
            l."end",
            l.reason,
            l.status,
            l.submitted_at,
            l.decided_at,
            l.decided_by_actor_id,
            l.created_at,
            l.updated_at,
            u.cid,
            u.display_name
        from org.loas l
        join identity.users u on u.id = l.user_id
        where ($1::text is null or l.status = $1)
          and ($2::bigint is null or u.cid = $2)
        order by l.start desc, l.created_at desc, l.id asc
        limit $3 offset $4
        "#,
    )
    .bind(status.as_deref())
    .bind(query.cid)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(LoaListResponse { items, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(patch, path = "/api/v1/admin/loa/{loa_id}/decision", tag = "workflows", params(("loa_id" = String, Path, description = "LOA ID")), request_body = DecideLoaRequest, responses((status = 200, description = "Updated LOA decision", body = LoaItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn decide_loa(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(loa_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<DecideLoaRequest>,
) -> Result<Json<LoaItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let normalized = normalize_loa_admin_status(&payload.status)?;
    let before = fetch_loa_by_id(pool, &loa_id).await?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;

    let row = sqlx::query_as::<_, LoaItem>(
        r#"
        update org.loas l
        set status = $2,
            decided_at = now(),
            decided_by_actor_id = $3,
            updated_at = now()
        from identity.users u
        where l.id = $1
          and u.id = l.user_id
        returning
            l.id,
            l.user_id,
            l.start,
            l."end",
            l.reason,
            l.status,
            l.submitted_at,
            l.decided_at,
            l.decided_by_actor_id,
            l.created_at,
            l.updated_at,
            u.cid,
            u.display_name
        "#,
    )
    .bind(&loa_id)
    .bind(normalized)
    .bind(actor.actor_id.as_deref())
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

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

    Ok(Json(row))
}

#[utoipa::path(post, path = "/api/v1/admin/loa/expire-run", tag = "workflows", responses((status = 200, description = "LOA expiration job run", body = JobRunResponse), (status = 401, description = "Not authenticated")))]
pub async fn run_loa_expiration(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
) -> Result<Json<JobRunResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let run_id = create_job_run(pool, "loa_expiration").await?;
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

    Ok(Json(JobRunResponse { run }))
}

#[utoipa::path(get, path = "/api/v1/users/{cid}/solo-certifications", tag = "workflows", params(("cid" = i64, Path, description = "User CID"), PaginationQuery), responses((status = 200, description = "User solo certifications", body = SoloCertificationListResponse), (status = 401, description = "Not authenticated")))]
pub async fn get_user_solo_certifications(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<SoloCertificationListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
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
    let (items, total) =
        list_solo_certifications(pool, None, Some(cid), pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(SoloCertificationListResponse { items, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(get, path = "/api/v1/admin/solo-certifications", tag = "workflows", params(PaginationQuery, ("cid" = Option<i64>, Query, description = "Optional user CID filter")), responses((status = 200, description = "Solo certification list", body = SoloCertificationListResponse), (status = 401, description = "Not authenticated")))]
pub async fn admin_list_solo_certifications(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListSoloCertificationsQuery>,
) -> Result<Json<SoloCertificationListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let (items, total) =
        list_solo_certifications(pool, query.cid, query.cid, pagination.page_size, pagination.offset).await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(SoloCertificationListResponse { items, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(post, path = "/api/v1/admin/solo-certifications", tag = "workflows", request_body = CreateSoloCertificationRequest, responses((status = 201, description = "Solo certification created", body = SoloCertificationItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_solo_certification(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateSoloCertificationRequest>,
) -> Result<(StatusCode, Json<SoloCertificationItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    if payload.position.trim().is_empty() || payload.expires <= Utc::now() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    ensure_solo_certification_type(pool, &payload.certification_type_id).await?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;

    let row = sqlx::query_as::<_, SoloCertificationItem>(
        r#"
        insert into org.user_solo_certifications (
            id,
            user_id,
            certification_type_id,
            position,
            expires,
            granted_at,
            granted_by_actor_id
        )
        values ($1, $2, $3, $4, $5, now(), $6)
        returning
            id,
            user_id,
            certification_type_id,
            position,
            expires,
            granted_at,
            granted_by_actor_id,
            null::bigint as cid,
            null::text as display_name,
            null::text as certification_type_name
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&payload.user_id)
    .bind(&payload.certification_type_id)
    .bind(payload.position.trim())
    .bind(payload.expires)
    .bind(actor.actor_id.as_deref())
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    let full = fetch_solo_certification(pool, &row.id).await?;
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

    Ok((StatusCode::CREATED, Json(full)))
}

#[utoipa::path(patch, path = "/api/v1/admin/solo-certifications/{solo_id}", tag = "workflows", params(("solo_id" = String, Path, description = "Solo certification ID")), request_body = UpdateSoloCertificationRequest, responses((status = 200, description = "Updated solo certification", body = SoloCertificationItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_solo_certification(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(solo_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateSoloCertificationRequest>,
) -> Result<Json<SoloCertificationItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_solo_certification(pool, &solo_id).await?;

    if let Some(certification_type_id) = payload.certification_type_id.as_deref() {
        ensure_solo_certification_type(pool, certification_type_id).await?;
    }
    if let Some(expires) = payload.expires {
        if expires <= Utc::now() {
            return Err(ApiError::BadRequest);
        }
    }

    let row = sqlx::query_as::<_, SoloCertificationItem>(
        r#"
        update org.user_solo_certifications
        set certification_type_id = coalesce($2, certification_type_id),
            position = coalesce($3, position),
            expires = coalesce($4, expires)
        where id = $1
        returning
            id,
            user_id,
            certification_type_id,
            position,
            expires,
            granted_at,
            granted_by_actor_id,
            null::bigint as cid,
            null::text as display_name,
            null::text as certification_type_name
        "#,
    )
    .bind(&solo_id)
    .bind(payload.certification_type_id.as_deref())
    .bind(
        payload
            .position
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )
    .bind(payload.expires)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let full = fetch_solo_certification(pool, &row.id).await?;
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

    Ok(Json(full))
}

#[utoipa::path(delete, path = "/api/v1/admin/solo-certifications/{solo_id}", tag = "workflows", params(("solo_id" = String, Path, description = "Solo certification ID")), responses((status = 200, description = "Deleted solo certification", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn delete_solo_certification(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(solo_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_solo_certification(pool, &solo_id).await?;
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;

    sqlx::query("delete from org.user_solo_certifications where id = $1")
        .bind(&solo_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

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
    Query(query): Query<PaginationQuery>,
) -> Result<Json<StaffingRequestListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination = query.resolve(25, 200);
    let total = sqlx::query_scalar::<_, i64>("select count(*)::bigint from org.staffing_requests where user_id = $1")
        .bind(&user.id)
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    let rows = sqlx::query_as::<_, StaffingRequestItem>(
        r#"
        select
            sr.id,
            sr.user_id,
            sr.name,
            sr.description,
            sr.created_at,
            sr.updated_at,
            u.cid,
            u.display_name
        from org.staffing_requests sr
        join identity.users u on u.id = sr.user_id
        where sr.user_id = $1
        order by sr.created_at desc, sr.id asc
        limit $2 offset $3
        "#,
    )
    .bind(&user.id)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(StaffingRequestListResponse { items: rows, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(post, path = "/api/v1/staffing-requests/me", tag = "workflows", request_body = CreateStaffingRequestRequest, responses((status = 201, description = "Staffing request created", body = StaffingRequestItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_staffing_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateStaffingRequestRequest>,
) -> Result<(StatusCode, Json<StaffingRequestItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let name = payload.name.trim();
    let description = payload.description.trim();
    if name.is_empty() || description.is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = sqlx::query_as::<_, StaffingRequestItem>(
        r#"
        insert into org.staffing_requests (id, user_id, name, description, created_at, updated_at)
        values ($1, $2, $3, $4, now(), now())
        returning
            id,
            user_id,
            name,
            description,
            created_at,
            updated_at,
            null::bigint as cid,
            null::text as display_name
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&user.id)
    .bind(name)
    .bind(description)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    let full = fetch_staffing_request(pool, &row.id).await?;
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

    Ok((StatusCode::CREATED, Json(full)))
}

#[utoipa::path(get, path = "/api/v1/admin/staffing-requests", tag = "workflows", params(PaginationQuery, ("cid" = Option<i64>, Query, description = "Optional user CID filter")), responses((status = 200, description = "Staffing request list", body = StaffingRequestListResponse), (status = 401, description = "Not authenticated")))]
pub async fn admin_list_staffing_requests(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListStaffingRequestsQuery>,
) -> Result<Json<StaffingRequestListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);

    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.staffing_requests sr
        join identity.users u on u.id = sr.user_id
        where ($1::bigint is null or u.cid = $1)
        "#,
    )
    .bind(query.cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let items = sqlx::query_as::<_, StaffingRequestItem>(
        r#"
        select
            sr.id,
            sr.user_id,
            sr.name,
            sr.description,
            sr.created_at,
            sr.updated_at,
            u.cid,
            u.display_name
        from org.staffing_requests sr
        join identity.users u on u.id = sr.user_id
        where ($1::bigint is null or u.cid = $1)
        order by sr.created_at desc, sr.id asc
        limit $2 offset $3
        "#,
    )
    .bind(query.cid)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(StaffingRequestListResponse { items, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(delete, path = "/api/v1/admin/staffing-requests/{request_id}", tag = "workflows", params(("request_id" = String, Path, description = "Staffing request ID")), responses((status = 200, description = "Deleted staffing request", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn delete_staffing_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_staffing_request(pool, &request_id).await?;

    sqlx::query("delete from org.staffing_requests where id = $1")
        .bind(&request_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

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
    Query(query): Query<PaginationQuery>,
) -> Result<Json<SuaListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination = query.resolve(25, 200);
    let (items, total) = list_sua_blocks(pool, Some(user.cid), pagination.page_size, pagination.offset).await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(SuaListResponse { items, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(post, path = "/api/v1/sua/me", tag = "workflows", request_body = CreateSuaRequest, responses((status = 201, description = "SUA request created", body = SuaBlockItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_sua_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<CreateSuaRequest>,
) -> Result<(StatusCode, Json<SuaBlockItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    validate_sua_request(&payload)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let active_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.sua_blocks
        where user_id = $1
          and end_at >= now()
        "#,
    )
    .bind(&user.id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    if active_count >= SUA_MAX_ACTIVE_REQUESTS {
        return Err(ApiError::BadRequest);
    }

    let mission_number = generate_mission_number(payload.start_at);
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into org.sua_blocks (
            id,
            user_id,
            start_at,
            end_at,
            afiliation,
            details,
            mission_number,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, now(), now())
        "#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(payload.start_at)
    .bind(payload.end_at)
    .bind(payload.afiliation.trim())
    .bind(payload.details.trim())
    .bind(&mission_number)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    for airspace in &payload.airspace {
        sqlx::query(
            r#"
            insert into org.sua_block_airspace (
                id,
                sua_block_id,
                identifier,
                bottom_altitude,
                top_altitude
            )
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&id)
        .bind(airspace.identifier.trim().to_ascii_uppercase())
        .bind(airspace.bottom_altitude.trim())
        .bind(airspace.top_altitude.trim())
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::BadRequest)?;
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    let item = fetch_sua_block(pool, &id).await?;

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

    Ok((StatusCode::CREATED, Json(item)))
}

#[utoipa::path(delete, path = "/api/v1/sua/{mission_id}", tag = "workflows", params(("mission_id" = String, Path, description = "SUA mission ID")), responses((status = 200, description = "Deleted SUA request", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn delete_sua_request(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(mission_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let before = fetch_sua_block(pool, &mission_id).await?;
    if before.user_id != user.id {
        return Err(ApiError::Unauthorized);
    }

    sqlx::query("delete from org.sua_blocks where id = $1 and user_id = $2")
        .bind(&mission_id)
        .bind(&user.id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

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
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListSuaQuery>,
) -> Result<Json<SuaListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "directory"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let (items, total) = list_sua_blocks(pool, query.cid, pagination.page_size, pagination.offset).await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(SuaListResponse { items, total: meta.total, page: meta.page, page_size: meta.page_size, total_pages: meta.total_pages, has_next: meta.has_next, has_prev: meta.has_prev }))
}

#[utoipa::path(patch, path = "/api/v1/admin/users/{cid}/controller-lifecycle", tag = "workflows", params(("cid" = i64, Path, description = "User CID")), request_body = ControllerLifecycleRequest, responses((status = 200, description = "Updated controller lifecycle", body = ControllerLifecycleResponse), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_controller_lifecycle(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    headers: HeaderMap,
    Json(payload): Json<ControllerLifecycleRequest>,
) -> Result<Json<ControllerLifecycleResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let normalized_status = normalize_controller_status(&payload.controller_status)?;
    let before = fetch_membership_lifecycle_row(pool, cid).await?;
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

    sqlx::query(
        r#"
        update org.memberships
        set controller_status = $2,
            artcc = coalesce($3, artcc),
            updated_at = now()
        where user_id = $1
        "#,
    )
    .bind(&before.user_id)
    .bind(normalized_status)
    .bind(artcc.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    if normalized_status == "NONE" {
        let result = sqlx::query(
            r#"
            update org.memberships
            set operating_initials = null,
                updated_at = now()
            where user_id = $1
              and operating_initials is not null
            "#,
        )
        .bind(&before.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;
        cleanup.operating_initials_cleared = result.rows_affected() > 0;

        if cleanup_on_none {
            cleanup.training_assignment_requests_deleted = delete_count(
                &mut tx,
                "delete from training.training_assignment_requests where student_id = $1",
                &before.user_id,
            )
            .await?;
            cleanup.training_assignments_deleted = delete_count(
                &mut tx,
                "delete from training.training_assignments where student_id = $1",
                &before.user_id,
            )
            .await?;
            cleanup.loas_deleted = delete_count(
                &mut tx,
                "delete from org.loas where user_id = $1",
                &before.user_id,
            )
            .await?;
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
            let result = sqlx::query(
                "update identity.user_profiles set show_welcome_message = true, updated_at = now() where user_id = $1",
            )
            .bind(&before.user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;
            cleanup.welcome_message_enabled = result.rows_affected() > 0;
        }
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    let after = fetch_membership_lifecycle_row(pool, cid).await?;
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

    Ok(Json(response))
}

#[utoipa::path(get, path = "/api/v1/admin/jobs", tag = "workflows", responses((status = 200, description = "Backend jobs", body = [JobStatusItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_jobs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<JobStatusItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["system"], PermissionAction::Read),
    )
    .await?;
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
    Ok(Json(items))
}

#[utoipa::path(get, path = "/api/v1/admin/jobs/{job_name}", tag = "workflows", params(("job_name" = String, Path, description = "Job name")), responses((status = 200, description = "Backend job detail", body = JobDetailResponse), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn get_job(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(job_name): Path<String>,
) -> Result<Json<JobDetailResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["system"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let health = state
        .job_health
        .read()
        .map_err(|_| ApiError::Internal)?
        .clone();
    let status = build_job_status(pool, &health, &job_name).await?;
    let recent_runs = sqlx::query_as::<_, JobRunItem>(
        r#"
        select id, job_name, started_at, finished_at, status, result_summary, error_text, created_at
        from platform.job_runs
        where job_name = $1
        order by started_at desc
        limit 10
        "#,
    )
    .bind(&job_name)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(Json(JobDetailResponse {
        status,
        recent_runs,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/jobs/{job_name}/run", tag = "workflows", params(("job_name" = String, Path, description = "Job name")), responses((status = 200, description = "Triggered backend job", body = JobRunResponse), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn run_job(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(job_name): Path<String>,
    headers: HeaderMap,
) -> Result<Json<JobRunResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["users", "controller_status"], PermissionAction::Update),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let run_id = create_job_run(pool, &job_name).await?;
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

    Ok(Json(JobRunResponse { run }))
}

async fn build_job_status(
    pool: &PgPool,
    health: &JobHealth,
    job_name: &str,
) -> Result<JobStatusItem, ApiError> {
    let latest_run = sqlx::query_as::<_, JobRunItem>(
        r#"
        select id, job_name, started_at, finished_at, status, result_summary, error_text, created_at
        from platform.job_runs
        where job_name = $1
        order by started_at desc
        limit 1
        "#,
    )
    .bind(job_name)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

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
    pool: &PgPool,
    actor: audit_repo::AuditActor,
) -> Result<JobExecutionSummary, ApiError> {
    let items = sqlx::query_as::<_, LoaItem>(
        r#"
        select
            l.id,
            l.user_id,
            l.start,
            l."end",
            l.reason,
            l.status,
            l.submitted_at,
            l.decided_at,
            l.decided_by_actor_id,
            l.created_at,
            l.updated_at,
            u.cid,
            u.display_name
        from org.loas l
        join identity.users u on u.id = l.user_id
        where l.status = 'APPROVED'
          and l."end" < now()
        order by l."end" asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut processed = 0;
    for item in &items {
        sqlx::query(
            "update org.loas set status = 'INACTIVE', updated_at = now() where id = $1 and status = 'APPROVED'",
        )
        .bind(&item.id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
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
    pool: &PgPool,
    actor: audit_repo::AuditActor,
) -> Result<JobExecutionSummary, ApiError> {
    let items = sqlx::query_as::<_, SoloCertificationItem>(
        r#"
        select
            s.id,
            s.user_id,
            s.certification_type_id,
            s.position,
            s.expires,
            s.granted_at,
            s.granted_by_actor_id,
            u.cid,
            u.display_name,
            ct.name as certification_type_name
        from org.user_solo_certifications s
        join identity.users u on u.id = s.user_id
        left join org.certification_types ct on ct.id = s.certification_type_id
        where s.expires < now()
        order by s.expires asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut processed = 0;
    for item in &items {
        sqlx::query("delete from org.user_solo_certifications where id = $1")
            .bind(&item.id)
            .execute(pool)
            .await
            .map_err(|_| ApiError::Internal)?;
        maybe_send_solo_email(state, pool, actor.clone(), "solo.expired", item, None).await;
        processed += 1;
    }

    Ok(JobExecutionSummary {
        processed,
        details: json!({ "expired_solo_ids": items.into_iter().map(|item| item.id).collect::<Vec<_>>() }),
    })
}

async fn execute_event_automation(pool: &PgPool) -> Result<JobExecutionSummary, ApiError> {
    let lock_threshold = Utc::now() + Duration::hours(24);
    let locked = sqlx::query(
        r#"
        update events.events
        set positions_locked = true,
            updated_at = now()
        where manual_positions_open = false
          and positions_locked = false
          and starts_at <= $1
          and archived_at is null
        "#,
    )
    .bind(lock_threshold)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .rows_affected() as i64;

    let archived = sqlx::query(
        r#"
        update events.events
        set archived_at = coalesce(archived_at, now()),
            hidden = true,
            positions_locked = true,
            manual_positions_open = false,
            banner_asset_id = null,
            status = 'ARCHIVED',
            updated_at = now()
        where ends_at <= $1
          and archived_at is null
        "#,
    )
    .bind(Utc::now() - Duration::hours(24))
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .rows_affected() as i64;

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
    pool: &PgPool,
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
    pool: &PgPool,
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

async fn create_job_run(pool: &PgPool, job_name: &str) -> Result<String, ApiError> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into platform.job_runs (id, job_name, started_at, status, created_at)
        values ($1, $2, now(), 'running', now())
        "#,
    )
    .bind(&id)
    .bind(job_name)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(id)
}

async fn finish_job_run(
    pool: &PgPool,
    run_id: &str,
    result: Result<JobExecutionSummary, ApiError>,
) -> Result<JobRunItem, ApiError> {
    match result {
        Ok(summary) => {
            sqlx::query(
                r#"
                update platform.job_runs
                set finished_at = now(),
                    status = 'succeeded',
                    result_summary = $2
                where id = $1
                "#,
            )
            .bind(run_id)
            .bind(json!({
                "processed": summary.processed,
                "details": summary.details
            }))
            .execute(pool)
            .await
            .map_err(|_| ApiError::Internal)?;
        }
        Err(err) => {
            sqlx::query(
                r#"
                update platform.job_runs
                set finished_at = now(),
                    status = 'failed',
                    error_text = $2
                where id = $1
                "#,
            )
            .bind(run_id)
            .bind(err.to_string())
            .execute(pool)
            .await
            .map_err(|_| ApiError::Internal)?;
        }
    }

    sqlx::query_as::<_, JobRunItem>(
        r#"
        select id, job_name, started_at, finished_at, status, result_summary, error_text, created_at
        from platform.job_runs
        where id = $1
        "#,
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
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

async fn fetch_loa_owned_by(
    pool: &PgPool,
    loa_id: &str,
    user_id: &str,
) -> Result<LoaItem, ApiError> {
    sqlx::query_as::<_, LoaItem>(
        r#"
        select
            id,
            user_id,
            start,
            "end",
            reason,
            status,
            submitted_at,
            decided_at,
            decided_by_actor_id,
            created_at,
            updated_at,
            null::bigint as cid,
            null::text as display_name
        from org.loas
        where id = $1
          and user_id = $2
        "#,
    )
    .bind(loa_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)
}

async fn fetch_loa_by_id(pool: &PgPool, loa_id: &str) -> Result<LoaItem, ApiError> {
    sqlx::query_as::<_, LoaItem>(
        r#"
        select
            l.id,
            l.user_id,
            l.start,
            l."end",
            l.reason,
            l.status,
            l.submitted_at,
            l.decided_at,
            l.decided_by_actor_id,
            l.created_at,
            l.updated_at,
            u.cid,
            u.display_name
        from org.loas l
        join identity.users u on u.id = l.user_id
        where l.id = $1
        "#,
    )
    .bind(loa_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)
}

async fn ensure_solo_certification_type(
    pool: &PgPool,
    certification_type_id: &str,
) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        select exists(
            select 1
            from org.certification_types
            where id = $1
              and can_solo_cert = true
        )
        "#,
    )
    .bind(certification_type_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    if !exists {
        return Err(ApiError::BadRequest);
    }
    Ok(())
}

async fn fetch_solo_certification(
    pool: &PgPool,
    solo_id: &str,
) -> Result<SoloCertificationItem, ApiError> {
    sqlx::query_as::<_, SoloCertificationItem>(
        r#"
        select
            s.id,
            s.user_id,
            s.certification_type_id,
            s.position,
            s.expires,
            s.granted_at,
            s.granted_by_actor_id,
            u.cid,
            u.display_name,
            ct.name as certification_type_name
        from org.user_solo_certifications s
        join identity.users u on u.id = s.user_id
        left join org.certification_types ct on ct.id = s.certification_type_id
        where s.id = $1
        "#,
    )
    .bind(solo_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)
}

async fn list_solo_certifications(
    pool: &PgPool,
    filter_cid_for_total: Option<i64>,
    filter_cid_for_items: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SoloCertificationItem>, i64), ApiError> {
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.user_solo_certifications s
        join identity.users u on u.id = s.user_id
        where ($1::bigint is null or u.cid = $1)
        "#,
    )
    .bind(filter_cid_for_total)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let items = sqlx::query_as::<_, SoloCertificationItem>(
        r#"
        select
            s.id,
            s.user_id,
            s.certification_type_id,
            s.position,
            s.expires,
            s.granted_at,
            s.granted_by_actor_id,
            u.cid,
            u.display_name,
            ct.name as certification_type_name
        from org.user_solo_certifications s
        join identity.users u on u.id = s.user_id
        left join org.certification_types ct on ct.id = s.certification_type_id
        where ($1::bigint is null or u.cid = $1)
        order by s.expires asc, s.granted_at desc
        limit $2 offset $3
        "#,
    )
    .bind(filter_cid_for_items)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok((items, total))
}

async fn fetch_staffing_request(
    pool: &PgPool,
    request_id: &str,
) -> Result<StaffingRequestItem, ApiError> {
    sqlx::query_as::<_, StaffingRequestItem>(
        r#"
        select
            sr.id,
            sr.user_id,
            sr.name,
            sr.description,
            sr.created_at,
            sr.updated_at,
            u.cid,
            u.display_name
        from org.staffing_requests sr
        join identity.users u on u.id = sr.user_id
        where sr.id = $1
        "#,
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)
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

async fn list_sua_blocks(
    pool: &PgPool,
    cid: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<SuaBlockItem>, i64), ApiError> {
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.sua_blocks b
        join identity.users u on u.id = b.user_id
        where ($1::bigint is null or u.cid = $1)
        "#,
    )
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let rows = sqlx::query_as::<_, SuaBlockRow>(
        r#"
        select
            b.id,
            b.user_id,
            b.start_at,
            b.end_at,
            b.afiliation,
            b.details,
            b.mission_number,
            b.created_at,
            b.updated_at,
            u.cid,
            u.display_name
        from org.sua_blocks b
        join identity.users u on u.id = b.user_id
        where ($1::bigint is null or u.cid = $1)
        order by b.start_at desc, b.created_at desc
        limit $2 offset $3
        "#,
    )
    .bind(cid)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let block_ids = rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
    let airspace_rows = if block_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, SuaAirspaceItem>(
            r#"
            select id, sua_block_id, identifier, bottom_altitude, top_altitude
            from org.sua_block_airspace
            where sua_block_id = any($1)
            order by identifier asc
            "#,
        )
        .bind(&block_ids)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?
    };

    let mut airspace_by_block = std::collections::HashMap::<String, Vec<SuaAirspaceItem>>::new();
    for row in airspace_rows {
        airspace_by_block
            .entry(row.sua_block_id.clone())
            .or_default()
            .push(row);
    }

    let items = rows
        .into_iter()
        .map(|row| SuaBlockItem {
            id: row.id.clone(),
            user_id: row.user_id,
            start_at: row.start_at,
            end_at: row.end_at,
            afiliation: row.afiliation,
            details: row.details,
            mission_number: row.mission_number,
            created_at: row.created_at,
            updated_at: row.updated_at,
            cid: row.cid,
            display_name: row.display_name,
            airspace: airspace_by_block.remove(&row.id).unwrap_or_default(),
        })
        .collect();

    Ok((items, total))
}

async fn fetch_sua_block(pool: &PgPool, mission_id: &str) -> Result<SuaBlockItem, ApiError> {
    let row = sqlx::query_as::<_, SuaBlockRow>(
        r#"
        select
            b.id,
            b.user_id,
            b.start_at,
            b.end_at,
            b.afiliation,
            b.details,
            b.mission_number,
            b.created_at,
            b.updated_at,
            u.cid,
            u.display_name
        from org.sua_blocks b
        join identity.users u on u.id = b.user_id
        where b.id = $1
        "#,
    )
    .bind(mission_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let airspace = sqlx::query_as::<_, SuaAirspaceItem>(
        r#"
        select id, sua_block_id, identifier, bottom_altitude, top_altitude
        from org.sua_block_airspace
        where sua_block_id = $1
        order by identifier asc
        "#,
    )
    .bind(&row.id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(SuaBlockItem {
        id: row.id,
        user_id: row.user_id,
        start_at: row.start_at,
        end_at: row.end_at,
        afiliation: row.afiliation,
        details: row.details,
        mission_number: row.mission_number,
        created_at: row.created_at,
        updated_at: row.updated_at,
        cid: row.cid,
        display_name: row.display_name,
        airspace,
    })
}

async fn fetch_membership_lifecycle_row(
    pool: &PgPool,
    cid: i64,
) -> Result<MembershipLifecycleRow, ApiError> {
    sqlx::query_as::<_, MembershipLifecycleRow>(
        r#"
        select
            m.user_id,
            u.cid,
            m.controller_status,
            m.artcc,
            m.operating_initials,
            u.first_name,
            u.last_name,
            u.display_name,
            p.show_welcome_message
        from org.memberships m
        join identity.users u on u.id = m.user_id
        join identity.user_profiles p on p.user_id = u.id
        where u.cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)
}

async fn delete_count(
    tx: &mut Transaction<'_, Postgres>,
    sql: &str,
    user_id: &str,
) -> Result<i64, ApiError> {
    let result = sqlx::query(sql)
        .bind(user_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(result.rows_affected() as i64)
}

async fn record_simple_audit(
    pool: &PgPool,
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
    pool: &PgPool,
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

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}
