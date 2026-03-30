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
            status
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
            status
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

