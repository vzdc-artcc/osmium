use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{errors::ApiError, models::SoloCertificationItem};

#[derive(Debug, sqlx::FromRow)]
struct SoloCertificationRow {
    id: String,
    user_id: String,
    certification_type_id: String,
    position: String,
    expires: DateTime<Utc>,
    granted_at: DateTime<Utc>,
    granted_by_actor_id: Option<String>,
    cid: Option<i64>,
    display_name: Option<String>,
    certification_type_name: Option<String>,
}

impl From<SoloCertificationRow> for SoloCertificationItem {
    fn from(row: SoloCertificationRow) -> Self {
        SoloCertificationItem {
            id: row.id,
            user_id: row.user_id,
            certification_type_id: row.certification_type_id,
            position: row.position,
            expires: row.expires,
            granted_at: row.granted_at,
            granted_by_actor_id: row.granted_by_actor_id,
            cid: row.cid,
            display_name: row.display_name,
            certification_type_name: row.certification_type_name,
        }
    }
}

pub async fn ensure_solo_certification_type_exists(
    pool: &PgPool,
    certification_type_id: &str,
) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, bool>(
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
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_solo_certification(
    pool: &PgPool,
    id: &str,
    user_id: &str,
    certification_type_id: &str,
    position: &str,
    expires: DateTime<Utc>,
    granted_by_actor_id: Option<&str>,
) -> Result<SoloCertificationItem, ApiError> {
    sqlx::query_as::<_, SoloCertificationRow>(
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
    .bind(id)
    .bind(user_id)
    .bind(certification_type_id)
    .bind(position)
    .bind(expires)
    .bind(granted_by_actor_id)
    .fetch_one(pool)
    .await
    .map(Into::into)
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_solo_certification(
    pool: &PgPool,
    solo_id: &str,
) -> Result<Option<SoloCertificationItem>, ApiError> {
    sqlx::query_as::<_, SoloCertificationRow>(
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
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn update_solo_certification_row(
    pool: &PgPool,
    solo_id: &str,
    certification_type_id: Option<&str>,
    position: Option<&str>,
    expires: Option<DateTime<Utc>>,
) -> Result<Option<SoloCertificationItem>, ApiError> {
    sqlx::query_as::<_, SoloCertificationRow>(
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
    .bind(solo_id)
    .bind(certification_type_id)
    .bind(position)
    .bind(expires)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Into::into))
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_solo_certification_row(pool: &PgPool, solo_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from org.user_solo_certifications where id = $1")
        .bind(solo_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn list_solo_certifications(
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

    let items = sqlx::query_as::<_, SoloCertificationRow>(
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
    .map(|rows| rows.into_iter().map(Into::into).collect::<Vec<_>>())
    .map_err(|_| ApiError::Internal)?;

    Ok((items, total))
}

pub async fn list_expired_solo_certifications(
    pool: &PgPool,
) -> Result<Vec<SoloCertificationItem>, ApiError> {
    sqlx::query_as::<_, SoloCertificationRow>(
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
    .map(|rows| rows.into_iter().map(Into::into).collect())
    .map_err(|_| ApiError::Internal)
}
