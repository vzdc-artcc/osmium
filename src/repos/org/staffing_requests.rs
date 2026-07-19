use sqlx::PgPool;

use crate::{errors::ApiError, models::StaffingRequestItem};

pub async fn count_my_staffing_requests(pool: &PgPool, user_id: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from org.staffing_requests where user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_my_staffing_requests(
    pool: &PgPool,
    user_id: &str,
    page_size: i64,
    offset: i64,
) -> Result<Vec<StaffingRequestItem>, ApiError> {
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
        where sr.user_id = $1
        order by sr.created_at desc, sr.id asc
        limit $2 offset $3
        "#,
    )
    .bind(user_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_staffing_request(
    pool: &PgPool,
    id: &str,
    user_id: &str,
    name: &str,
    description: &str,
) -> Result<StaffingRequestItem, ApiError> {
    sqlx::query_as::<_, StaffingRequestItem>(
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
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::BadRequest)
}

pub async fn fetch_staffing_request(
    pool: &PgPool,
    request_id: &str,
) -> Result<Option<StaffingRequestItem>, ApiError> {
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
    .map_err(|_| ApiError::Internal)
}

pub async fn count_admin_staffing_requests(
    pool: &PgPool,
    cid: Option<i64>,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from org.staffing_requests sr
        join identity.users u on u.id = sr.user_id
        where ($1::bigint is null or u.cid = $1)
        "#,
    )
    .bind(cid)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_admin_staffing_requests(
    pool: &PgPool,
    cid: Option<i64>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<StaffingRequestItem>, ApiError> {
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
        where ($1::bigint is null or u.cid = $1)
        order by sr.created_at desc, sr.id asc
        limit $2 offset $3
        "#,
    )
    .bind(cid)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_staffing_request_row(pool: &PgPool, request_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from org.staffing_requests where id = $1")
        .bind(request_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}
