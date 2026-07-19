use sqlx::{Executor, PgPool, Postgres};

use crate::{errors::ApiError, models::media::FileAuditLogItem};

#[derive(Clone, sqlx::FromRow)]
pub struct FileAssetRow {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub etag: String,
    pub storage_key: String,
    pub is_public: bool,
    pub uploaded_by: String,
    pub owner_user_id: Option<String>,
    pub viewer_roles: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<FileAssetRow> for crate::models::media::FileAsset {
    fn from(row: FileAssetRow) -> Self {
        Self {
            id: row.id,
            filename: row.filename,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            etag: row.etag,
            is_public: row.is_public,
            uploaded_by: row.uploaded_by,
            owner_user_id: row.owner_user_id,
            viewer_roles: row.viewer_roles,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn count_audit_logs(pool: &PgPool, file_id: Option<&str>) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from media.file_audit_logs where ($1::text is null or file_id = $1)",
    )
    .bind(file_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_audit_logs(
    pool: &PgPool,
    file_id: Option<&str>,
    page_size: i64,
    offset: i64,
) -> Result<Vec<FileAuditLogItem>, ApiError> {
    sqlx::query_as::<_, FileAuditLogItem>(
        r#"
        select
            id,
            action,
            file_id,
            actor_user_id,
            ip_address,
            outcome,
            coalesce(details, '{}'::jsonb) as details,
            created_at
        from media.file_audit_logs
        where ($1::text is null or file_id = $1)
        order by created_at desc, id asc
        limit $2 offset $3
        "#,
    )
    .bind(file_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_audit_log(
    pool: &PgPool,
    id: &str,
    action: &str,
    file_id: Option<&str>,
    actor_user_id: Option<&str>,
    ip_address: &str,
    outcome: &str,
    details: serde_json::Value,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into media.file_audit_logs (
            id,
            action,
            file_id,
            actor_user_id,
            ip_address,
            outcome,
            details,
            created_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, now())
        "#,
    )
    .bind(id)
    .bind(action)
    .bind(file_id)
    .bind(actor_user_id)
    .bind(ip_address)
    .bind(outcome)
    .bind(details)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn count_visible_files(
    pool: &PgPool,
    user_id: &str,
    roles: &[String],
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from media.file_assets fa
        where fa.is_public
           or fa.uploaded_by = $1
           or fa.owner_user_id = $1
           or fa.viewer_roles && $2::text[]
           or exists (
                select 1
                from media.file_asset_allowed_users au
                where au.file_id = fa.id
                  and au.user_id = $1
           )
        "#,
    )
    .bind(user_id)
    .bind(roles)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_visible_files(
    pool: &PgPool,
    user_id: &str,
    roles: &[String],
    page_size: i64,
    offset: i64,
) -> Result<Vec<FileAssetRow>, ApiError> {
    sqlx::query_as::<_, FileAssetRow>(
        r#"
        select
            fa.id,
            fa.filename,
            fa.content_type,
            fa.size_bytes,
            fa.etag,
            fa.storage_key,
            fa.is_public,
            fa.uploaded_by,
            fa.owner_user_id,
            fa.viewer_roles,
            fa.created_at,
            fa.updated_at
        from media.file_assets fa
        where fa.is_public
           or fa.uploaded_by = $1
           or fa.owner_user_id = $1
           or fa.viewer_roles && $2::text[]
           or exists (
                select 1
                from media.file_asset_allowed_users au
                where au.file_id = fa.id
                  and au.user_id = $1
           )
        order by fa.created_at desc, fa.id asc
        limit $3 offset $4
        "#,
    )
    .bind(user_id)
    .bind(roles)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_file_asset(
    pool: &PgPool,
    id: &str,
    filename: &str,
    content_type: &str,
    size_bytes: i64,
    etag: &str,
    storage_key: &str,
    is_public: bool,
    uploaded_by: &str,
    owner_user_id: Option<&str>,
    viewer_roles: &[String],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<FileAssetRow, ApiError> {
    sqlx::query_as::<_, FileAssetRow>(
        r#"
        insert into media.file_assets (
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            owner_user_id,
            viewer_roles,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
        returning
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            owner_user_id,
            viewer_roles,
            created_at,
            updated_at
        "#,
    )
    .bind(id)
    .bind(filename)
    .bind(content_type)
    .bind(size_bytes)
    .bind(etag)
    .bind(storage_key)
    .bind(is_public)
    .bind(uploaded_by)
    .bind(owner_user_id)
    .bind(viewer_roles)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_allowed_user<'e, E>(
    executor: E,
    file_id: &str,
    user_id: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        insert into media.file_asset_allowed_users (file_id, user_id)
        values ($1, $2)
        on conflict (file_id, user_id) do nothing
        "#,
    )
    .bind(file_id)
    .bind(user_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_file_row(
    pool: &PgPool,
    file_id: &str,
) -> Result<Option<FileAssetRow>, ApiError> {
    sqlx::query_as::<_, FileAssetRow>(
        r#"
        select
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            owner_user_id,
            viewer_roles,
            created_at,
            updated_at
        from media.file_assets
        where id = $1
        "#,
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn count_direct_user_access(
    pool: &PgPool,
    file_id: &str,
    user_id: &str,
) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from media.file_asset_allowed_users
        where file_id = $1 and user_id = $2
        "#,
    )
    .bind(file_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn resolve_user_id_by_cid(pool: &PgPool, cid: i64) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>("select id from identity.users where cid = $1")
        .bind(cid)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn resolve_user_ids_by_cids(
    pool: &PgPool,
    cids: &[i64],
) -> Result<Vec<(i64, String)>, ApiError> {
    if cids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, (i64, String)>("select cid, id from identity.users where cid = any($1)")
        .bind(cids)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn update_file_metadata<'e, E>(
    executor: E,
    file_id: &str,
    filename: Option<&str>,
    content_type: Option<&str>,
    is_public: Option<bool>,
    owner_user_id: Option<&str>,
    viewer_roles: Option<&[String]>,
) -> Result<Option<FileAssetRow>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, FileAssetRow>(
        r#"
        update media.file_assets
        set filename = coalesce($1, filename),
            content_type = coalesce($2, content_type),
            is_public = coalesce($3, is_public),
            owner_user_id = coalesce($4, owner_user_id),
            viewer_roles = coalesce($5, viewer_roles),
            updated_at = now()
        where id = $6
        returning
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            owner_user_id,
            viewer_roles,
            created_at,
            updated_at
        "#,
    )
    .bind(filename)
    .bind(content_type)
    .bind(is_public)
    .bind(owner_user_id)
    .bind(viewer_roles)
    .bind(file_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn clear_allowed_users<'e, E>(executor: E, file_id: &str) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query("delete from media.file_asset_allowed_users where file_id = $1")
        .bind(file_id)
        .execute(executor)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn replace_file_content_row(
    pool: &PgPool,
    file_id: &str,
    filename: &str,
    content_type: &str,
    size_bytes: i64,
    etag: &str,
) -> Result<FileAssetRow, ApiError> {
    sqlx::query_as::<_, FileAssetRow>(
        r#"
        update media.file_assets
        set filename = $1,
            content_type = $2,
            size_bytes = $3,
            etag = $4,
            updated_at = now()
        where id = $5
        returning
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            owner_user_id,
            viewer_roles,
            created_at,
            updated_at
        "#,
    )
    .bind(filename)
    .bind(content_type)
    .bind(size_bytes)
    .bind(etag)
    .bind(file_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn delete_file_asset(pool: &PgPool, file_id: &str) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "delete from media.file_assets where id = $1 returning storage_key",
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
