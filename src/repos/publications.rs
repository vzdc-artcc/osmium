use sqlx::{Executor, PgPool, Postgres};

use crate::{
    errors::ApiError,
    models::{Publication, PublicationCategory},
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PublicationRow {
    pub id: String,
    pub category_id: String,
    pub category_key: String,
    pub category_name: String,
    pub title: String,
    pub description: Option<String>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub file_id: String,
    pub file_filename: String,
    pub file_content_type: String,
    pub file_size_bytes: i64,
    pub is_public: bool,
    pub sort_order: i32,
    pub status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PublicationRecord {
    pub id: String,
    pub category_id: String,
    pub title: String,
    pub description: Option<String>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub file_id: String,
    pub is_public: bool,
    pub sort_order: i32,
    pub status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FileAssetLinkRow {
    pub is_public: bool,
    pub domain_type: Option<String>,
    pub domain_id: Option<String>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<PublicationRow> for Publication {
    fn from(row: PublicationRow) -> Self {
        Self {
            id: row.id,
            category_id: row.category_id,
            category_key: row.category_key,
            category_name: row.category_name,
            title: row.title,
            description: row.description,
            effective_at: row.effective_at,
            updated_at: row.updated_at,
            file_id: row.file_id.clone(),
            cdn_url: format!("/cdn/{}", row.file_id),
            file_filename: row.file_filename,
            file_content_type: row.file_content_type,
            file_size_bytes: row.file_size_bytes,
            is_public: row.is_public,
            sort_order: row.sort_order,
            status: row.status,
        }
    }
}

pub async fn fetch_publication_categories(
    pool: &PgPool,
) -> Result<Vec<PublicationCategory>, ApiError> {
    sqlx::query_as::<_, PublicationCategory>(
        r#"
        select id, key, name, description, sort_order, created_at, updated_at
        from web.publication_categories
        order by sort_order asc, name asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn insert_category<'e, E>(
    executor: E,
    id: &str,
    key: &str,
    name: &str,
    description: Option<&str>,
    sort_order: i32,
) -> Result<PublicationCategory, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, PublicationCategory>(
        r#"
        insert into web.publication_categories (id, key, name, description, sort_order)
        values ($1, $2, $3, $4, $5)
        returning id, key, name, description, sort_order, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(key)
    .bind(name)
    .bind(description)
    .bind(sort_order)
    .fetch_one(executor)
    .await
    .map_err(super::map_constraint_error)
}

pub async fn update_category<'e, E>(
    executor: E,
    category_id: &str,
    key: Option<&str>,
    name: Option<&str>,
    description: Option<&str>,
    sort_order: Option<i32>,
) -> Result<Option<PublicationCategory>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, PublicationCategory>(
        r#"
        update web.publication_categories
        set
            key = coalesce($1, key),
            name = coalesce($2, name),
            description = coalesce($3, description),
            sort_order = coalesce($4, sort_order)
        where id = $5
        returning id, key, name, description, sort_order, created_at, updated_at
        "#,
    )
    .bind(key)
    .bind(name)
    .bind(description)
    .bind(sort_order)
    .bind(category_id)
    .fetch_optional(executor)
    .await
    .map_err(super::map_constraint_error)
}

pub async fn delete_category(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    category_id: &str,
) -> Result<u64, ApiError> {
    let result = sqlx::query("delete from web.publication_categories where id = $1")
        .bind(category_id)
        .execute(&mut **tx)
        .await
        .map_err(super::map_constraint_error)?;

    Ok(result.rows_affected())
}

pub async fn fetch_publications(
    pool: &PgPool,
    public_only: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<PublicationRow>, ApiError> {
    let query = if public_only {
        r#"
        select
            p.id,
            p.category_id,
            c.key as category_key,
            c.name as category_name,
            p.title,
            p.description,
            p.effective_at,
            p.updated_at,
            p.file_id,
            fa.filename as file_filename,
            fa.content_type as file_content_type,
            fa.size_bytes as file_size_bytes,
            p.is_public,
            p.sort_order,
            p.status
        from web.publications p
        join web.publication_categories c on c.id = p.category_id
        join media.file_assets fa on fa.id = p.file_id
        where p.is_public = true
          and p.status = 'published'
          and p.effective_at <= now()
          and fa.deleted_at is null
          and fa.is_public = true
        order by c.sort_order asc, p.sort_order asc, p.effective_at desc, p.title asc, p.id asc
        limit $1 offset $2
        "#
    } else {
        r#"
        select
            p.id,
            p.category_id,
            c.key as category_key,
            c.name as category_name,
            p.title,
            p.description,
            p.effective_at,
            p.updated_at,
            p.file_id,
            fa.filename as file_filename,
            fa.content_type as file_content_type,
            fa.size_bytes as file_size_bytes,
            p.is_public,
            p.sort_order,
            p.status
        from web.publications p
        join web.publication_categories c on c.id = p.category_id
        join media.file_assets fa on fa.id = p.file_id
        where fa.deleted_at is null
        order by c.sort_order asc, p.sort_order asc, p.effective_at desc, p.title asc, p.id asc
        limit $1 offset $2
        "#
    };

    sqlx::query_as::<_, PublicationRow>(query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn count_publications(pool: &PgPool, public_only: bool) -> Result<i64, ApiError> {
    let query = if public_only {
        r#"
        select count(*)::bigint
        from web.publications p
        join media.file_assets fa on fa.id = p.file_id
        where p.is_public = true
          and p.status = 'published'
          and p.effective_at <= now()
          and fa.deleted_at is null
          and fa.is_public = true
        "#
    } else {
        r#"
        select count(*)::bigint
        from web.publications p
        join media.file_assets fa on fa.id = p.file_id
        where fa.deleted_at is null
        "#
    };

    sqlx::query_scalar::<_, i64>(query)
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn fetch_publication(
    pool: &PgPool,
    publication_id: &str,
    public_only: bool,
) -> Result<Option<PublicationRow>, ApiError> {
    let query = if public_only {
        r#"
        select
            p.id,
            p.category_id,
            c.key as category_key,
            c.name as category_name,
            p.title,
            p.description,
            p.effective_at,
            p.updated_at,
            p.file_id,
            fa.filename as file_filename,
            fa.content_type as file_content_type,
            fa.size_bytes as file_size_bytes,
            p.is_public,
            p.sort_order,
            p.status
        from web.publications p
        join web.publication_categories c on c.id = p.category_id
        join media.file_assets fa on fa.id = p.file_id
        where p.id = $1
          and p.is_public = true
          and p.status = 'published'
          and p.effective_at <= now()
          and fa.deleted_at is null
          and fa.is_public = true
        "#
    } else {
        r#"
        select
            p.id,
            p.category_id,
            c.key as category_key,
            c.name as category_name,
            p.title,
            p.description,
            p.effective_at,
            p.updated_at,
            p.file_id,
            fa.filename as file_filename,
            fa.content_type as file_content_type,
            fa.size_bytes as file_size_bytes,
            p.is_public,
            p.sort_order,
            p.status
        from web.publications p
        join web.publication_categories c on c.id = p.category_id
        join media.file_assets fa on fa.id = p.file_id
        where p.id = $1
          and fa.deleted_at is null
        "#
    };

    sqlx::query_as::<_, PublicationRow>(query)
        .bind(publication_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

pub async fn fetch_publication_in_tx<'e, E>(
    executor: E,
    publication_id: &str,
) -> Result<Option<PublicationRow>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, PublicationRow>(
        r#"
        select
            p.id,
            p.category_id,
            c.key as category_key,
            c.name as category_name,
            p.title,
            p.description,
            p.effective_at,
            p.updated_at,
            p.file_id,
            fa.filename as file_filename,
            fa.content_type as file_content_type,
            fa.size_bytes as file_size_bytes,
            p.is_public,
            p.sort_order,
            p.status
        from web.publications p
        join web.publication_categories c on c.id = p.category_id
        join media.file_assets fa on fa.id = p.file_id
        where p.id = $1
          and fa.deleted_at is null
        "#,
    )
    .bind(publication_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_publication_record_for_update<'e, E>(
    executor: E,
    publication_id: &str,
) -> Result<Option<PublicationRecord>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, PublicationRecord>(
        r#"
        select
            id,
            category_id,
            title,
            description,
            effective_at,
            updated_at,
            file_id,
            is_public,
            sort_order,
            status
        from web.publications
        where id = $1
        for update
        "#,
    )
    .bind(publication_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_publication_category_for_update<'e, E>(
    executor: E,
    category_id: &str,
) -> Result<Option<PublicationCategory>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, PublicationCategory>(
        r#"
        select id, key, name, description, sort_order, created_at, updated_at
        from web.publication_categories
        where id = $1
        for update
        "#,
    )
    .bind(category_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn ensure_category_exists<'e, E>(executor: E, category_id: &str) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    let exists = sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from web.publication_categories where id = $1)",
    )
    .bind(category_id)
    .fetch_one(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    if exists {
        Ok(())
    } else {
        Err(ApiError::BadRequest)
    }
}

pub async fn fetch_file_asset_for_update<'e, E>(
    executor: E,
    file_id: &str,
) -> Result<Option<FileAssetLinkRow>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, FileAssetLinkRow>(
        r#"
        select is_public, domain_type, domain_id, deleted_at
        from media.file_assets
        where id = $1
        for update
        "#,
    )
    .bind(file_id)
    .fetch_optional(executor)
    .await
    .map_err(|_| ApiError::Internal)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_publication<'e, E>(
    executor: E,
    id: &str,
    category_id: &str,
    title: &str,
    description: Option<&str>,
    effective_at: chrono::DateTime<chrono::Utc>,
    file_id: &str,
    is_public: bool,
    sort_order: i32,
    status: &str,
) -> Result<PublicationRecord, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, PublicationRecord>(
        r#"
        insert into web.publications (
            id,
            category_id,
            title,
            description,
            effective_at,
            file_id,
            is_public,
            sort_order,
            status
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        returning
            id,
            category_id,
            title,
            description,
            effective_at,
            file_id,
            is_public,
            sort_order,
            status
        "#,
    )
    .bind(id)
    .bind(category_id)
    .bind(title)
    .bind(description)
    .bind(effective_at)
    .bind(file_id)
    .bind(is_public)
    .bind(sort_order)
    .bind(status)
    .fetch_one(executor)
    .await
    .map_err(super::map_constraint_error)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_publication_row<'e, E>(
    executor: E,
    publication_id: &str,
    category_id: &str,
    title: &str,
    description: Option<&str>,
    effective_at: chrono::DateTime<chrono::Utc>,
    file_id: &str,
    is_public: bool,
    sort_order: i32,
    status: &str,
) -> Result<Option<PublicationRecord>, ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, PublicationRecord>(
        r#"
        update web.publications
        set
            category_id = $1,
            title = $2,
            description = $3,
            effective_at = $4,
            file_id = $5,
            is_public = $6,
            sort_order = $7,
            status = $8
        where id = $9
        returning
            id,
            category_id,
            title,
            description,
            effective_at,
            updated_at,
            file_id,
            is_public,
            sort_order,
            status
        "#,
    )
    .bind(category_id)
    .bind(title)
    .bind(description)
    .bind(effective_at)
    .bind(file_id)
    .bind(is_public)
    .bind(sort_order)
    .bind(status)
    .bind(publication_id)
    .fetch_optional(executor)
    .await
    .map_err(super::map_constraint_error)
}

pub async fn delete_publication_row(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    publication_id: &str,
) -> Result<u64, ApiError> {
    let result = sqlx::query("delete from web.publications where id = $1")
        .bind(publication_id)
        .execute(&mut **tx)
        .await
        .map_err(super::map_constraint_error)?;

    Ok(result.rows_affected())
}

pub async fn attach_file_to_publication<'e, E>(
    executor: E,
    file_id: &str,
    publication_id: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        update media.file_assets
        set domain_type = 'publication', domain_id = $1
        where id = $2
        "#,
    )
    .bind(publication_id)
    .bind(file_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn detach_file_from_publication<'e, E>(
    executor: E,
    file_id: &str,
    publication_id: &str,
) -> Result<(), ApiError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        update media.file_assets
        set domain_type = null, domain_id = null
        where id = $1
          and domain_type = 'publication'
          and domain_id = $2
        "#,
    )
    .bind(file_id)
    .bind(publication_id)
    .execute(executor)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}
