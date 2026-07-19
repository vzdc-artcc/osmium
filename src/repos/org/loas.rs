use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres};

use crate::{errors::ApiError, models::LoaItem};

#[derive(Debug, sqlx::FromRow)]
struct LoaRow {
    id: String,
    user_id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    reason: String,
    status: String,
    submitted_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
    decided_by_actor_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    cid: Option<i64>,
    display_name: Option<String>,
}

impl From<LoaRow> for LoaItem {
    fn from(row: LoaRow) -> Self {
        LoaItem {
            id: row.id,
            user_id: row.user_id,
            start: row.start,
            end: row.end,
            reason: row.reason,
            status: row.status,
            submitted_at: row.submitted_at,
            decided_at: row.decided_at,
            decided_by_actor_id: row.decided_by_actor_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            cid: row.cid,
            display_name: row.display_name,
        }
    }
}

pub async fn count_my_loas(pool: &PgPool, user_id: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>("select count(*)::bigint from org.loas where user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn list_my_loas(
    pool: &PgPool,
    user_id: &str,
    page_size: i64,
    offset: i64,
) -> Result<Vec<LoaItem>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
        r#"
        select id, user_id, start, "end", reason, status, submitted_at, decided_at, decided_by_actor_id, created_at, updated_at, null::bigint as cid, null::text as display_name
        from org.loas
        where user_id = $1
        order by start desc, created_at desc, id asc
        limit $2 offset $3
        "#,
    )
    .bind(user_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_loa(
    pool: &PgPool,
    id: &str,
    user_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    reason: &str,
) -> Result<LoaItem, ApiError> {
    sqlx::query_as::<_, LoaRow>(
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
    .bind(id)
    .bind(user_id)
    .bind(start)
    .bind(end)
    .bind(reason)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_loa_owned_by(
    pool: &PgPool,
    loa_id: &str,
    user_id: &str,
) -> Result<Option<LoaItem>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
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
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn update_loa_row(
    pool: &PgPool,
    loa_id: &str,
    user_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    reason: &str,
) -> Result<Option<LoaItem>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
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
    .bind(loa_id)
    .bind(user_id)
    .bind(start)
    .bind(end)
    .bind(reason)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn count_admin_loas(
    pool: &PgPool,
    status: Option<&str>,
    cid: Option<i64>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.loas l
        join identity.users u on u.id = l.user_id
        where ($1::text is null or l.status = $1)
          and ($2::bigint is null or u.cid = $2)
        "#,
    )
    .bind(status)
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_admin_loas(
    pool: &PgPool,
    status: Option<&str>,
    cid: Option<i64>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<LoaItem>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
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
    .bind(status)
    .bind(cid)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_loa_by_id(pool: &PgPool, loa_id: &str) -> Result<Option<LoaItem>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
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
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn decide_loa_row(
    pool: &PgPool,
    loa_id: &str,
    status: &str,
    decided_by_actor_id: Option<&str>,
) -> Result<Option<LoaItem>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
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
    .bind(loa_id)
    .bind(status)
    .bind(decided_by_actor_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn list_expired_approved_loas(pool: &PgPool) -> Result<Vec<LoaItem>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
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
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}

pub async fn expire_loa_row(pool: &PgPool, loa_id: &str) -> Result<(), ApiError> {
    sqlx::query(
        "update org.loas set status = 'INACTIVE', updated_at = now() where id = $1 and status = 'APPROVED'",
    )
    .bind(loa_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn delete_loas_for_user<'e, E>(executor: E, user_id: &str) -> Result<i64, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let result = sqlx::query("delete from org.loas where user_id = $1")
        .bind(user_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(result.rows_affected() as i64)
}
