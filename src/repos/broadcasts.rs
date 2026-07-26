use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Executor, PgPool, Postgres};

use crate::{
    errors::ApiError,
    models::{
        BroadcastRecipientItem, ChangeBroadcastDetail, ChangeBroadcastListItem,
        MyChangeBroadcastItem,
    },
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BroadcastRow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub file_id: Option<String>,
    pub exempt_staff: bool,
    pub timestamp: DateTime<Utc>,
}

pub async fn count_broadcasts(
    pool: &PgPool,
    title_pattern: Option<&str>,
    exempt_staff: Option<bool>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from web.change_broadcasts cb
        where ($1::text is null or cb.title ilike $1)
          and ($2::bool is null or cb.exempt_staff = $2)
        "#,
    )
    .bind(title_pattern)
    .bind(exempt_staff)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_broadcasts(
    pool: &PgPool,
    title_pattern: Option<&str>,
    exempt_staff: Option<bool>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<ChangeBroadcastListItem>, ApiError> {
    sqlx::query_as::<_, ChangeBroadcastListItem>(
        r#"
        select
            cb.id,
            cb.title,
            cb.description,
            cb.file_id,
            fa.filename as file_filename,
            cb.exempt_staff,
            cb.timestamp,
            cb.updated_at,
            (
                select count(*)::bigint
                from web.change_broadcast_user_state s
                where s.broadcast_id = cb.id and s.seen_at is not null
            ) as seen_count,
            (
                select count(*)::bigint
                from web.change_broadcast_user_state s
                where s.broadcast_id = cb.id and s.agreed_at is not null
            ) as agreed_count
        from web.change_broadcasts cb
        left join media.file_assets fa on fa.id = cb.file_id
        where ($1::text is null or cb.title ilike $1)
          and ($2::bool is null or cb.exempt_staff = $2)
        order by cb.timestamp desc, cb.id asc
        limit $3 offset $4
        "#,
    )
    .bind(title_pattern)
    .bind(exempt_staff)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_broadcast_list_item(
    pool: &PgPool,
    broadcast_id: &str,
) -> Result<Option<ChangeBroadcastListItem>, ApiError> {
    sqlx::query_as::<_, ChangeBroadcastListItem>(
        r#"
        select
            cb.id,
            cb.title,
            cb.description,
            cb.file_id,
            fa.filename as file_filename,
            cb.exempt_staff,
            cb.timestamp,
            cb.updated_at,
            (
                select count(*)::bigint
                from web.change_broadcast_user_state s
                where s.broadcast_id = cb.id and s.seen_at is not null
            ) as seen_count,
            (
                select count(*)::bigint
                from web.change_broadcast_user_state s
                where s.broadcast_id = cb.id and s.agreed_at is not null
            ) as agreed_count
        from web.change_broadcasts cb
        left join media.file_assets fa on fa.id = cb.file_id
        where cb.id = $1
        "#,
    )
    .bind(broadcast_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_broadcast_row<'e, E>(
    executor: E,
    broadcast_id: &str,
) -> Result<Option<BroadcastRow>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, BroadcastRow>(
        r#"
        select id, title, description, file_id, exempt_staff, timestamp
        from web.change_broadcasts
        where id = $1
        "#,
    )
    .bind(broadcast_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_broadcast<'e, E>(
    executor: E,
    id: &str,
    title: &str,
    description: &str,
    file_id: Option<&str>,
    exempt_staff: bool,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into web.change_broadcasts (
            id, title, description, file_id, exempt_staff, timestamp, created_at, updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $6, $6)
        "#,
    )
    .bind(id)
    .bind(title)
    .bind(description)
    .bind(file_id)
    .bind(exempt_staff)
    .bind(now)
    .execute(executor)
    .await
    .map_err(super::map_constraint_error)?;
    Ok(())
}

pub async fn update_broadcast_row<'e, E>(
    executor: E,
    broadcast_id: &str,
    title: &str,
    description: &str,
    file_id: Option<&str>,
    exempt_staff: bool,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        update web.change_broadcasts
        set title = $2,
            description = $3,
            file_id = $4,
            exempt_staff = $5,
            timestamp = $6,
            updated_at = $6
        where id = $1
        "#,
    )
    .bind(broadcast_id)
    .bind(title)
    .bind(description)
    .bind(file_id)
    .bind(exempt_staff)
    .bind(now)
    .execute(executor)
    .await
    .map_err(super::map_constraint_error)?;
    Ok(())
}

pub async fn delete_broadcast_row<'e, E>(executor: E, broadcast_id: &str) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query("delete from web.change_broadcasts where id = $1")
        .bind(broadcast_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn insert_staff_agreed_state<'e, E>(
    executor: E,
    broadcast_id: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into web.change_broadcast_user_state (broadcast_id, user_id, seen_at, agreed_at)
        select $1, ur.user_id, $2, $2
        from access.user_roles ur
        where ur.role_name = 'STAFF'
        on conflict (broadcast_id, user_id) do nothing
        "#,
    )
    .bind(broadcast_id)
    .bind(now)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

/// Resolves the fixed set of named recipient groups the website's broadcast
/// picker offers (mirroring the live site's precomputed "MailGroup" cohorts)
/// into a concrete, deduplicated set of internal user ids. Every group is
/// implicitly scoped to users who currently hold an active controller status
/// (HOME or VISITOR) and have email notifications enabled — same base pool
/// the live site's own group computation started from. Returns
/// `ApiError::BadRequest` for any key outside the fixed set below, since
/// these come from a closed client-side dropdown, not free-form user input.
pub async fn resolve_recipient_group_user_ids(
    pool: &PgPool,
    groups: &[String],
) -> Result<Vec<String>, ApiError> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }

    let mut all_rostered = false;
    let mut visiting = false;
    let mut home_ratings: Vec<&str> = Vec::new();
    let mut role_names: Vec<&str> = Vec::new();

    for group in groups {
        match group.as_str() {
            "ALL" => all_rostered = true,
            "HOME_OBS" => home_ratings.push("OBS"),
            "HOME_S1" => home_ratings.push("S1"),
            "HOME_S2" => home_ratings.push("S2"),
            "HOME_S3" => home_ratings.push("S3"),
            "HOME_C1_C3" => home_ratings.extend(["C1", "C2", "C3"]),
            "VISITING" => visiting = true,
            "INSTRUCTORS" => role_names.push("INS"),
            "MENTORS" => role_names.push("MTR"),
            "ALL_TRAINING_STAFF" => role_names.extend(["INS", "MTR"]),
            _ => return Err(ApiError::BadRequest),
        }
    }

    let home_ratings: Option<Vec<&str>> = (!home_ratings.is_empty()).then_some(home_ratings);
    let role_names: Option<Vec<&str>> = (!role_names.is_empty()).then_some(role_names);

    sqlx::query_scalar::<_, String>(
        r#"
        select distinct u.id
        from identity.users u
        left join org.memberships m on m.user_id = u.id
        left join identity.user_profiles p on p.user_id = u.id
        left join access.user_roles ur on ur.user_id = u.id
        where coalesce(m.controller_status, 'NONE') <> 'NONE'
          and coalesce(p.receive_email, true) = true
          and (
            $1::bool
            or ($2::text[] is not null and m.controller_status = 'HOME' and m.rating = any($2))
            or ($3::bool and m.controller_status = 'VISITOR')
            or ($4::text[] is not null and ur.role_name = any($4))
          )
        "#,
    )
    .bind(all_rostered)
    .bind(home_ratings)
    .bind(visiting)
    .bind(role_names)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_recipients<'e, E>(
    executor: E,
    broadcast_id: &str,
    user_ids: &[String],
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    if user_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"
        insert into web.change_broadcast_recipients (broadcast_id, user_id)
        select $1, unnest($2::text[])
        on conflict (broadcast_id, user_id) do nothing
        "#,
    )
    .bind(broadcast_id)
    .bind(user_ids)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

/// Also adds STAFF-role users as recipients (in addition to whatever
/// explicit list was chosen) so an exempt-staff broadcast still shows up,
/// pre-agreed, in their own broadcast history — "exempt" means exempt from
/// having to act, not invisible.
pub async fn insert_staff_recipients<'e, E>(executor: E, broadcast_id: &str) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into web.change_broadcast_recipients (broadcast_id, user_id)
        select $1, ur.user_id
        from access.user_roles ur
        where ur.role_name = 'STAFF'
        on conflict (broadcast_id, user_id) do nothing
        "#,
    )
    .bind(broadcast_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn is_recipient(
    pool: &PgPool,
    broadcast_id: &str,
    user_id: &str,
) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, bool>(
        r#"
        select exists(
            select 1 from web.change_broadcast_recipients
            where broadcast_id = $1 and user_id = $2
        )
        "#,
    )
    .bind(broadcast_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_broadcast_detail(
    pool: &PgPool,
    broadcast_id: &str,
) -> Result<Option<ChangeBroadcastDetail>, ApiError> {
    let Some(item) = fetch_broadcast_list_item(pool, broadcast_id).await? else {
        return Ok(None);
    };

    let recipients = sqlx::query_as::<_, BroadcastRecipientItem>(
        r#"
        select
            u.cid,
            u.display_name as name,
            s.seen_at,
            s.agreed_at
        from web.change_broadcast_recipients r
        join identity.users u on u.id = r.user_id
        left join web.change_broadcast_user_state s
            on s.broadcast_id = r.broadcast_id and s.user_id = r.user_id
        where r.broadcast_id = $1
        order by u.display_name asc
        "#,
    )
    .bind(broadcast_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Some(ChangeBroadcastDetail {
        id: item.id,
        title: item.title,
        description: item.description,
        file_id: item.file_id,
        file_filename: item.file_filename,
        exempt_staff: item.exempt_staff,
        timestamp: item.timestamp,
        updated_at: item.updated_at,
        recipients,
    }))
}

pub async fn fetch_my_broadcasts(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<MyChangeBroadcastItem>, ApiError> {
    sqlx::query_as::<_, MyChangeBroadcastItem>(
        r#"
        select
            cb.id,
            cb.title,
            cb.description,
            cb.file_id,
            fa.filename as file_filename,
            cb.timestamp,
            s.seen_at,
            s.agreed_at
        from web.change_broadcasts cb
        join web.change_broadcast_recipients r on r.broadcast_id = cb.id and r.user_id = $1
        left join media.file_assets fa on fa.id = cb.file_id
        left join web.change_broadcast_user_state s
            on s.broadcast_id = cb.id and s.user_id = $1
        order by cb.timestamp desc, cb.id asc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn upsert_seen_state(
    pool: &PgPool,
    broadcast_id: &str,
    user_id: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into web.change_broadcast_user_state (broadcast_id, user_id, seen_at)
        values ($1, $2, $3)
        on conflict (broadcast_id, user_id) do update
        set seen_at = coalesce(web.change_broadcast_user_state.seen_at, excluded.seen_at)
        "#,
    )
    .bind(broadcast_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(super::map_constraint_error)?;
    Ok(())
}

pub async fn upsert_agreed_state(
    pool: &PgPool,
    broadcast_id: &str,
    user_id: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into web.change_broadcast_user_state (broadcast_id, user_id, seen_at, agreed_at)
        values ($1, $2, $3, $3)
        on conflict (broadcast_id, user_id) do update
        set seen_at = coalesce(web.change_broadcast_user_state.seen_at, excluded.seen_at),
            agreed_at = excluded.agreed_at
        "#,
    )
    .bind(broadcast_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(super::map_constraint_error)?;
    Ok(())
}
