use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Executor, PgPool, Postgres};

use crate::{
    errors::ApiError,
    models::{ChangeBroadcastListItem, MyChangeBroadcastItem},
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
