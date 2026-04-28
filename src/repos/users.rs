use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    errors::ApiError,
    models::users::{AdminUserListItem, RosterUserRow, UserStats},
};

pub async fn list_admin_users(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<AdminUserListItem>, ApiError> {
    sqlx::query_as::<_, AdminUserListItem>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status
        from org.v_user_roster_profile
        order by cid asc
        limit $1 offset $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_admin_user_by_cid(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<AdminUserListItem>, ApiError> {
    sqlx::query_as::<_, AdminUserListItem>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status
        from org.v_user_roster_profile
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_roster_users(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<RosterUserRow>, ApiError> {
    sqlx::query_as::<_, RosterUserRow>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status,
            controller_status
        from org.v_user_roster_profile
        order by cid asc
        limit $1 offset $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_roster_user_by_cid(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<RosterUserRow>, ApiError> {
    sqlx::query_as::<_, RosterUserRow>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status,
            controller_status
        from org.v_user_roster_profile
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn update_controller_status(
    pool: &PgPool,
    cid: i64,
    controller_status: &str,
    artcc: Option<&str>,
) -> Result<Option<(i64, String, Option<String>)>, ApiError> {
    sqlx::query_as::<_, (i64, String, Option<String>)>(
        r#"
        update org.memberships
        set controller_status = $2,
            artcc = coalesce($3, artcc),
            updated_at = now()
        where user_id = (select id from identity.users where cid = $1)
        returning
            (select cid from identity.users where id = org.memberships.user_id) as cid,
            controller_status,
            artcc
        "#,
    )
    .bind(cid)
    .bind(controller_status)
    .bind(artcc)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_user_stats(pool: &PgPool, user_id: &str) -> Result<UserStats, ApiError> {
    sqlx::query_as::<_, UserStats>(
        r#"
        select
            (select count(*)::bigint from identity.sessions s where s.user_id = $1 and s.revoked_at is null and s.expires_at > now()) as active_sessions,
            (select count(*)::bigint from events.event_positions ep where ep.user_id = $1) as assigned_event_positions,
            (select count(*)::bigint from training.training_assignments ta where ta.student_id = $1) as training_assignments_as_student,
            (select count(*)::bigint from training.training_assignments ta where ta.primary_trainer_id = $1) as training_assignments_as_primary_trainer,
            (select count(*)::bigint from training.training_assignment_other_trainers taot where taot.trainer_id = $1) as training_assignments_as_other_trainer,
            (select count(*)::bigint from training.training_assignment_requests tar where tar.student_id = $1) as training_assignment_requests,
            (select count(*)::bigint from training.training_assignment_request_interested_trainers tarit where tarit.trainer_id = $1) as training_assignment_interests,
            (select count(*)::bigint from training.trainer_release_requests trr where trr.student_id = $1) as trainer_release_requests
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn ensure_visitor_membership(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    artcc: &str,
    rating: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into org.memberships (
            user_id,
            artcc,
            rating,
            division,
            controller_status,
            membership_status,
            is_active,
            updated_at
        )
        values ($3, $1, $2, 'USA', 'VISITOR', 'ACTIVE', true, now())
        on conflict (user_id) do update
        set artcc = excluded.artcc,
            rating = coalesce(excluded.rating, org.memberships.rating),
            membership_status = 'ACTIVE',
            is_active = true,
            updated_at = now()
        "#,
    )
    .bind(artcc)
    .bind(rating)
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_user_cid_artcc_rating(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<(i64, Option<String>, Option<String>), ApiError> {
    sqlx::query_as::<_, (i64, Option<String>, Option<String>)>(
        "select u.cid, m.artcc, m.rating from identity.users u join org.memberships m on m.user_id = u.id where u.id = $1",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_user_identity_by_cid(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<(String, i64)>, ApiError> {
    sqlx::query_as::<_, (String, i64)>("select id, cid from identity.users where cid = $1")
        .bind(cid)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}
