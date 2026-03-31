use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{
        acl::{Permission, fetch_user_access},
        middleware::CurrentUser,
    },
    errors::ApiError,
    models::FeedbackItem,
    state::AppState,
};

#[derive(Deserialize)]
pub struct ListUsersQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    cid: i64,
    email: String,
    display_name: String,
    role: String,
    first_name: Option<String>,
    last_name: Option<String>,
    artcc: Option<String>,
    rating: Option<String>,
    division: Option<String>,
    status: Option<String>,
    controller_status: Option<String>,
}

#[derive(Serialize)]
pub struct UserBasicInfo {
    cid: i64,
    name: String,
    rating: Option<String>,
}

#[derive(Serialize)]
pub struct UserPrivateInfo {
    id: String,
    email: String,
    display_name: String,
    role: String,
    first_name: Option<String>,
    last_name: Option<String>,
    artcc: Option<String>,
    division: Option<String>,
    status: Option<String>,
    controller_status: Option<String>,
}

#[derive(Serialize)]
pub struct UserListItem {
    basic: UserBasicInfo,
    full: Option<UserPrivateInfo>,
}

#[derive(Serialize)]
pub struct UserDetailsResponse {
    basic: UserBasicInfo,
    full: Option<UserFullInfo>,
}

#[derive(Serialize)]
pub struct UserFullInfo {
    profile: UserPrivateInfo,
    roles: Vec<String>,
    permissions: Vec<Permission>,
    stats: UserStats,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct UserStats {
    active_sessions: i64,
    assigned_event_positions: i64,
    training_assignments_as_student: i64,
    training_assignments_as_primary_trainer: i64,
    training_assignments_as_other_trainer: i64,
    training_assignment_requests: i64,
    training_assignment_interests: i64,
    trainer_release_requests: i64,
}

#[derive(Deserialize)]
pub struct VisitArtccRequest {
    artcc: String,
    rating: Option<String>,
}

#[derive(Serialize)]
pub struct VisitArtccResponse {
    cid: i64,
    artcc: String,
    rating: Option<String>,
    status: String,
    roster_added: bool,
}

#[derive(Deserialize)]
pub struct UserFeedbackQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<Vec<UserListItem>>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let can_view_all = can_view_all_users(&state, viewer).await?;
    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows = sqlx::query_as::<_, UserRow>(
        r#"
        select
            id,
            cid,
            email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status,
            controller_status
        from users
        order by cid asc
        limit $1 offset $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let items = rows
        .into_iter()
        .map(|row| {
            let basic = basic_info_from_row(&row);
            let full = if can_view_all || row.cid == viewer.cid {
                Some(private_info_from_row(&row))
            } else {
                None
            };

            UserListItem { basic, full }
        })
        .collect();

    Ok(Json(items))
}

pub async fn get_user(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
) -> Result<Json<UserDetailsResponse>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = sqlx::query_as::<_, UserRow>(
        r#"
        select
            id,
            cid,
            email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status,
            controller_status
        from users
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let basic = basic_info_from_row(&row);
    let can_view_all = can_view_all_users(&state, viewer).await?;
    let can_view_full = can_view_all || row.cid == viewer.cid;

    if !can_view_full {
        return Ok(Json(UserDetailsResponse { basic, full: None }));
    }

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &row.id, &row.role).await?;
    let stats = fetch_user_stats(pool, &row.id).await?;

    Ok(Json(UserDetailsResponse {
        basic,
        full: Some(UserFullInfo {
            profile: private_info_from_row(&row),
            roles,
            permissions,
            stats,
        }),
    }))
}

pub async fn visit_artcc(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<VisitArtccRequest>,
) -> Result<Json<VisitArtccResponse>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let artcc = payload.artcc.trim().to_ascii_uppercase();
    if artcc.is_empty() || artcc.len() > 8 {
        return Err(ApiError::BadRequest);
    }

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        update users
        set artcc = $1,
            rating = coalesce($2, rating),
            status = 'ACTIVE',
            updated_at = now()
        where id = $3
        "#,
    )
    .bind(&artcc)
    .bind(payload.rating.as_deref())
    .bind(&viewer.id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into user_roles (user_id, role_name)
        values ($1, 'USER')
        on conflict (user_id, role_name) do nothing
        "#,
    )
    .bind(&viewer.id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let updated = sqlx::query_as::<_, (i64, Option<String>, Option<String>)>(
        "select cid, artcc, rating from users where id = $1",
    )
    .bind(&viewer.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(VisitArtccResponse {
        cid: updated.0,
        artcc: updated.1.unwrap_or(artcc),
        rating: updated.2,
        status: "ACTIVE".to_string(),
        roster_added: true,
    }))
}

pub async fn get_user_feedback(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    Query(query): Query<UserFeedbackQuery>,
) -> Result<Json<Vec<FeedbackItem>>, ApiError> {
    let viewer = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let target = sqlx::query_as::<_, (String, i64)>("select id, cid from users where cid = $1")
        .bind(cid)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::BadRequest)?;

    let can_view_all = can_view_all_users(&state, viewer).await?;
    if !can_view_all && target.1 != viewer.cid {
        return Err(ApiError::Unauthorized);
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
        from feedback_items
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

async fn can_view_all_users(state: &AppState, user: &CurrentUser) -> Result<bool, ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id, &user.role).await?;

    Ok(
        permissions.contains(&Permission::ManageUsers)
            || permissions.contains(&Permission::ViewAllUsers),
    )
}

async fn fetch_user_stats(pool: &sqlx::PgPool, user_id: &str) -> Result<UserStats, ApiError> {
    sqlx::query_as::<_, UserStats>(
        r#"
        select
            (select count(*)::bigint from sessions s where s.user_id = $1 and s.expires_at > now()) as active_sessions,
            (select count(*)::bigint from event_positions ep where ep.user_id = $1) as assigned_event_positions,
            (select count(*)::bigint from training_assignments ta where ta.student_id = $1) as training_assignments_as_student,
            (select count(*)::bigint from training_assignments ta where ta.primary_trainer_id = $1) as training_assignments_as_primary_trainer,
            (select count(*)::bigint from training_assignment_other_trainers taot where taot.trainer_id = $1) as training_assignments_as_other_trainer,
            (select count(*)::bigint from training_assignment_requests tar where tar.student_id = $1) as training_assignment_requests,
            (select count(*)::bigint from training_assignment_request_interested_trainers tarit where tarit.trainer_id = $1) as training_assignment_interests,
            (select count(*)::bigint from trainer_release_requests trr where trr.student_id = $1) as trainer_release_requests
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

fn basic_info_from_row(row: &UserRow) -> UserBasicInfo {
    UserBasicInfo {
        cid: row.cid,
        name: display_name(row),
        rating: row.rating.clone(),
    }
}

fn private_info_from_row(row: &UserRow) -> UserPrivateInfo {
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
    }
}

fn display_name(row: &UserRow) -> String {
    let first = row.first_name.clone().unwrap_or_default();
    let last = row.last_name.clone().unwrap_or_default();
    let joined = format!("{} {}", first.trim(), last.trim()).trim().to_string();

    if joined.is_empty() {
        row.display_name.clone()
    } else {
        joined
    }
}

