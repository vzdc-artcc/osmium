use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres};

use crate::{
    errors::ApiError,
    models::{SuaAirspaceItem, SuaBlockItem},
};

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

pub async fn count_active_sua_for_user(pool: &PgPool, user_id: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.sua_blocks
        where user_id = $1
          and end_at >= now()
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_sua_block<'e, E>(
    executor: E,
    id: &str,
    user_id: &str,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    afiliation: &str,
    details: &str,
    mission_number: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
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
    .bind(id)
    .bind(user_id)
    .bind(start_at)
    .bind(end_at)
    .bind(afiliation)
    .bind(details)
    .bind(mission_number)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn insert_sua_airspace<'e, E>(
    executor: E,
    id: &str,
    sua_block_id: &str,
    identifier: &str,
    bottom_altitude: &str,
    top_altitude: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
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
    .bind(id)
    .bind(sua_block_id)
    .bind(identifier)
    .bind(bottom_altitude)
    .bind(top_altitude)
    .execute(executor)
    .await
    .map_err(|_| ApiError::BadRequest)?;

    Ok(())
}

pub async fn fetch_sua_block(
    pool: &PgPool,
    mission_id: &str,
) -> Result<Option<SuaBlockItem>, ApiError> {
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
    .map_err(|_| ApiError::Internal)?;

    let Some(row) = row else {
        return Ok(None);
    };

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

    Ok(Some(SuaBlockItem {
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
    }))
}

pub async fn delete_sua_block_owned(
    pool: &PgPool,
    mission_id: &str,
    user_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("delete from org.sua_blocks where id = $1 and user_id = $2")
        .bind(mission_id)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn list_sua_blocks(
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
