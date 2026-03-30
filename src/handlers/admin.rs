use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{
        acl::{Permission, Role, fetch_access_catalog, fetch_user_access},
        middleware::{CurrentUser, ensure_permission},
    },
    errors::ApiError,
    state::AppState,
};

#[derive(Serialize)]
pub struct AclDebugBody {
    user_id: String,
    role: String,
    roles: Vec<String>,
    permissions: Vec<Permission>,
}

#[derive(Deserialize)]
pub struct UpdateUserAccessRequest {
    role: Option<String>,
    roles: Option<Vec<String>>,
    permissions: Option<Vec<PermissionOverrideInput>>,
}

#[derive(Deserialize)]
pub struct PermissionOverrideInput {
    name: String,
    granted: bool,
}

#[derive(Serialize)]
pub struct UserAccessBody {
    id: String,
    cid: i64,
    role: String,
    roles: Vec<String>,
    permissions: Vec<Permission>,
}

#[derive(Serialize)]
pub struct AccessCatalogBody {
    roles: Vec<String>,
    permissions: Vec<String>,
}

#[derive(Deserialize)]
pub struct ListUsersQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct UserListItem {
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

#[derive(sqlx::FromRow)]
struct UserProfileRow {
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
pub struct UserOverviewBody {
    user: UserListItem,
    roles: Vec<String>,
    permissions: Vec<Permission>,
    stats: UserOverviewStats,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct UserOverviewStats {
    active_sessions: i64,
    assigned_event_positions: i64,
    training_assignments_as_student: i64,
    training_assignments_as_primary_trainer: i64,
    training_assignments_as_other_trainer: i64,
    training_assignment_requests: i64,
    training_assignment_interests: i64,
    trainer_release_requests: i64,
}

pub async fn acl_debug(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<AclDebugBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ManageUsers).await?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id, &user.role).await?;

    Ok(Json(AclDebugBody {
        user_id: user.id.clone(),
        role: user.role.clone(),
        roles,
        permissions,
    }))
}

pub async fn get_user_access(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
) -> Result<Json<UserAccessBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ManageUsers).await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target = sqlx::query_as::<_, CurrentUser>(
        r#"
        select id, cid, email, display_name, role
        from users
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &target.id, &target.role).await?;
    Ok(Json(build_user_access_body(&target, roles, permissions)))
}

pub async fn get_access_catalog(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<AccessCatalogBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ManageUsers).await?;

    let (roles, permissions) = fetch_access_catalog(state.db.as_ref()).await?;
    Ok(Json(AccessCatalogBody { roles, permissions }))
}

pub async fn list_users(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<Vec<UserListItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ManageUsers).await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let users = sqlx::query_as::<_, UserListItem>(
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

    Ok(Json(users))
}

pub async fn get_user_overview(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
) -> Result<Json<UserOverviewBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ManageUsers).await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let target = sqlx::query_as::<_, UserProfileRow>(
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

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &target.id, &target.role).await?;

    let stats = sqlx::query_as::<_, UserOverviewStats>(
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
    .bind(&target.id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(UserOverviewBody {
        user: UserListItem {
            id: target.id,
            cid: target.cid,
            email: target.email,
            display_name: target.display_name,
            role: target.role,
            first_name: target.first_name,
            last_name: target.last_name,
            artcc: target.artcc,
            rating: target.rating,
            division: target.division,
            status: target.status,
        },
        roles,
        permissions,
        stats,
    }))
}

pub async fn update_user_access(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(cid): Path<i64>,
    Json(payload): Json<UpdateUserAccessRequest>,
) -> Result<Json<UserAccessBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ManageUsers).await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let parsed_roles = parse_roles(payload.roles.as_deref(), payload.role.as_deref())?;
    let parsed_permissions = parse_permissions(payload.permissions.as_deref())?;

    if parsed_roles.is_empty() && parsed_permissions.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let target_user_id = sqlx::query_scalar::<_, String>("select id from users where cid = $1")
        .bind(cid)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::BadRequest)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    if !parsed_roles.is_empty() {
        sqlx::query("delete from user_roles where user_id = $1")
            .bind(&target_user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;

        for role in &parsed_roles {
            sqlx::query(
                r#"
                insert into user_roles (user_id, role_name)
                values ($1, $2)
                on conflict (user_id, role_name) do nothing
                "#,
            )
            .bind(&target_user_id)
            .bind(role)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;
        }

        let primary_role = parsed_roles[0].as_str();
        sqlx::query("update users set role = $2, updated_at = now() where cid = $1")
            .bind(cid)
            .bind(primary_role)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;
    }

    if !parsed_permissions.is_empty() {
        sqlx::query("delete from user_permissions where user_id = $1")
            .bind(&target_user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;

        for (permission_name, granted) in &parsed_permissions {
            sqlx::query(
                r#"
                insert into user_permissions (user_id, permission_name, granted)
                values ($1, $2, $3)
                on conflict (user_id, permission_name) do update
                set granted = excluded.granted
                "#,
            )
            .bind(&target_user_id)
            .bind(permission_name)
            .bind(*granted)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;
        }
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let updated = sqlx::query_as::<_, CurrentUser>(
        r#"
        select id, cid, email, display_name, role
        from users
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &updated.id, &updated.role).await?;

    Ok(Json(build_user_access_body(&updated, roles, permissions)))
}

fn parse_roles(raw_roles: Option<&[String]>, raw_role: Option<&str>) -> Result<Vec<String>, ApiError> {
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
        let normalized = match Role::from_db_value(&role) {
            Some(Role::User) => "USER",
            Some(Role::Staff) => "STAFF",
            None => return Err(ApiError::BadRequest),
        };
        if !parsed.iter().any(|value| value == normalized) {
            parsed.push(normalized.to_string());
        }
    }

    Ok(parsed)
}

fn parse_permissions(
    raw_permissions: Option<&[PermissionOverrideInput]>,
) -> Result<Vec<(String, bool)>, ApiError> {
    let Some(raw_permissions) = raw_permissions else {
        return Ok(Vec::new());
    };

    let mut parsed: Vec<(String, bool)> = Vec::with_capacity(raw_permissions.len());
    for override_input in raw_permissions {
        let Some(permission) = Permission::from_db_value(&override_input.name) else {
            return Err(ApiError::BadRequest);
        };

        let normalized = permission.as_db_value().to_string();
        if let Some(existing) = parsed.iter_mut().find(|value| value.0 == normalized) {
            existing.1 = override_input.granted;
        } else {
            parsed.push((normalized, override_input.granted));
        }
    }

    Ok(parsed)
}

fn build_user_access_body(
    user: &CurrentUser,
    roles: Vec<String>,
    permissions: Vec<Permission>,
) -> UserAccessBody {
    UserAccessBody {
        id: user.id.clone(),
        cid: user.cid,
        role: user.role.clone(),
        roles,
        permissions,
    }
}

