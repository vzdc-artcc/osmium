use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionKey, PermissionResource},
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{
        CreatePublicationCategoryRequest, CreatePublicationRequest, Publication,
        PublicationCategory, UpdatePublicationCategoryRequest, UpdatePublicationRequest,
    },
    repos::audit as audit_repo,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct PublicationRow {
    id: String,
    category_id: String,
    category_key: String,
    category_name: String,
    title: String,
    description: Option<String>,
    effective_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    file_id: String,
    file_filename: String,
    file_content_type: String,
    file_size_bytes: i64,
    is_public: bool,
    sort_order: i32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PublicationRecord {
    id: String,
    category_id: String,
    title: String,
    description: Option<String>,
    effective_at: chrono::DateTime<chrono::Utc>,
    file_id: String,
    is_public: bool,
    sort_order: i32,
    status: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FileAssetLinkRow {
    is_public: bool,
    domain_type: Option<String>,
    domain_id: Option<String>,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
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

#[utoipa::path(
    get,
    path = "/api/v1/publications/categories",
    tag = "publications",
    responses(
        (status = 200, description = "List publication categories", body = [PublicationCategory])
    )
)]
pub async fn list_publication_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicationCategory>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let categories = fetch_publication_categories(pool).await?;
    Ok(Json(categories))
}

#[utoipa::path(
    get,
    path = "/api/v1/publications",
    tag = "publications",
    responses(
        (status = 200, description = "List public publications", body = [Publication])
    )
)]
pub async fn list_publications(
    State(state): State<AppState>,
) -> Result<Json<Vec<Publication>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = fetch_publications(pool, true).await?;
    Ok(Json(rows.into_iter().map(Publication::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/publications/{publication_id}",
    tag = "publications",
    params(
        ("publication_id" = String, Path, description = "Publication ID")
    ),
    responses(
        (status = 200, description = "Publication details", body = Publication),
        (status = 400, description = "Invalid publication ID")
    )
)]
pub async fn get_publication(
    State(state): State<AppState>,
    Path(publication_id): Path<String>,
) -> Result<Json<Publication>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let publication = fetch_publication(pool, &publication_id, true)
        .await?
        .ok_or(ApiError::BadRequest)?;
    Ok(Json(publication.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/publications/categories",
    tag = "publications",
    responses(
        (status = 200, description = "List publication categories for admin", body = [PublicationCategory]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn admin_list_publication_categories(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<Vec<PublicationCategory>>, ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let categories = fetch_publication_categories(pool).await?;
    Ok(Json(categories))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/publications/categories",
    tag = "publications",
    request_body = CreatePublicationCategoryRequest,
    responses(
        (status = 201, description = "Publication category created", body = PublicationCategory),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_publication_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    Json(payload): Json<CreatePublicationCategoryRequest>,
) -> Result<(StatusCode, Json<PublicationCategory>), ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let key = normalize_category_key(&payload.key)?;
    let name = normalize_required_text(&payload.name)?;
    let description = normalize_optional_text(payload.description);
    let sort_order = payload.sort_order.unwrap_or(0);

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let category = sqlx::query_as::<_, PublicationCategory>(
        r#"
        insert into web.publication_categories (id, key, name, description, sort_order)
        values ($1, $2, $3, $4, $5)
        returning id, key, name, description, sort_order, created_at, updated_at
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(key)
    .bind(name)
    .bind(description)
    .bind(sort_order)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_constraint_error)?;

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        AuditAction::Create,
        "PUBLICATION_CATEGORY",
        &category.id,
        None::<&PublicationCategory>,
        Some(&category),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok((StatusCode::CREATED, Json(category)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/publications/categories/{category_id}",
    tag = "publications",
    params(
        ("category_id" = String, Path, description = "Category ID")
    ),
    request_body = UpdatePublicationCategoryRequest,
    responses(
        (status = 200, description = "Publication category updated", body = PublicationCategory),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_publication_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePublicationCategoryRequest>,
) -> Result<Json<PublicationCategory>, ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before = fetch_publication_category_for_update(&mut tx, &category_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let key = match payload.key {
        Some(value) => Some(normalize_category_key(&value)?),
        None => None,
    };
    let name = match payload.name {
        Some(value) => Some(normalize_required_text(&value)?),
        None => None,
    };
    let description = payload.description.and_then(normalize_optional_text_value);

    let category = sqlx::query_as::<_, PublicationCategory>(
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
    .bind(payload.sort_order)
    .bind(&category_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_constraint_error)?
    .ok_or(ApiError::BadRequest)?;

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        AuditAction::Update,
        "PUBLICATION_CATEGORY",
        &category.id,
        Some(&before),
        Some(&category),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(Json(category))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/publications/categories/{category_id}",
    tag = "publications",
    params(
        ("category_id" = String, Path, description = "Category ID")
    ),
    responses(
        (status = 204, description = "Publication category deleted"),
        (status = 400, description = "Invalid category ID"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn delete_publication_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before = fetch_publication_category_for_update(&mut tx, &category_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let result = sqlx::query("delete from web.publication_categories where id = $1")
        .bind(&category_id)
        .execute(&mut *tx)
        .await
        .map_err(map_constraint_error)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest);
    }

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        AuditAction::Delete,
        "PUBLICATION_CATEGORY",
        &before.id,
        Some(&before),
        None::<&PublicationCategory>,
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/publications",
    tag = "publications",
    responses(
        (status = 200, description = "List publications for admin", body = [Publication]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn admin_list_publications(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<Vec<Publication>>, ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let rows = fetch_publications(pool, false).await?;
    Ok(Json(rows.into_iter().map(Publication::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/publications/{publication_id}",
    tag = "publications",
    params(
        ("publication_id" = String, Path, description = "Publication ID")
    ),
    responses(
        (status = 200, description = "Publication details for admin", body = Publication),
        (status = 400, description = "Invalid publication ID"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn admin_get_publication(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(publication_id): Path<String>,
) -> Result<Json<Publication>, ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let publication = fetch_publication(pool, &publication_id, false)
        .await?
        .ok_or(ApiError::BadRequest)?;
    Ok(Json(publication.into()))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/publications",
    tag = "publications",
    request_body = CreatePublicationRequest,
    responses(
        (status = 201, description = "Publication created", body = Publication),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn create_publication(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    Json(payload): Json<CreatePublicationRequest>,
) -> Result<(StatusCode, Json<Publication>), ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let publication_id = Uuid::new_v4().to_string();
    let input = NewPublicationInput {
        category_id: normalize_id(&payload.category_id)?,
        title: normalize_required_text(&payload.title)?,
        description: normalize_optional_text(payload.description),
        effective_at: payload.effective_at,
        file_id: normalize_id(&payload.file_id)?,
        is_public: payload.is_public,
        sort_order: payload.sort_order.unwrap_or(0),
        status: normalize_publication_status(&payload.status)?,
    };

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    ensure_category_exists(&mut tx, &input.category_id).await?;
    let file = fetch_file_asset_for_update(&mut tx, &input.file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    ensure_file_can_link(&file, None, input.is_public)?;

    let publication = sqlx::query_as::<_, PublicationRecord>(
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
    .bind(&publication_id)
    .bind(&input.category_id)
    .bind(&input.title)
    .bind(&input.description)
    .bind(input.effective_at)
    .bind(&input.file_id)
    .bind(input.is_public)
    .bind(input.sort_order)
    .bind(&input.status)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_constraint_error)?;

    attach_file_to_publication(&mut tx, &publication.file_id, &publication.id).await?;
    let response = fetch_publication_in_tx(&mut tx, &publication.id)
        .await?
        .ok_or(ApiError::Internal)?;
    let response = Publication::from(response);

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        AuditAction::Create,
        "PUBLICATION",
        &response.id,
        None::<&Publication>,
        Some(&response),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/publications/{publication_id}",
    tag = "publications",
    params(
        ("publication_id" = String, Path, description = "Publication ID")
    ),
    request_body = UpdatePublicationRequest,
    responses(
        (status = 200, description = "Publication updated", body = Publication),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_publication(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(publication_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePublicationRequest>,
) -> Result<Json<Publication>, ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before_row = fetch_publication_record_for_update(&mut tx, &publication_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let before = fetch_publication_in_tx(&mut tx, &publication_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let before = Publication::from(before);

    let category_id = match payload.category_id {
        Some(value) => normalize_id(&value)?,
        None => before_row.category_id.clone(),
    };
    ensure_category_exists(&mut tx, &category_id).await?;

    let title = match payload.title {
        Some(value) => normalize_required_text(&value)?,
        None => before_row.title.clone(),
    };
    let description = payload
        .description
        .and_then(normalize_optional_text_value)
        .or(before_row.description);
    let effective_at = payload.effective_at.unwrap_or(before_row.effective_at);
    let file_id = match payload.file_id {
        Some(value) => normalize_id(&value)?,
        None => before_row.file_id.clone(),
    };
    let is_public = payload.is_public.unwrap_or(before_row.is_public);
    let sort_order = payload.sort_order.unwrap_or(before_row.sort_order);
    let status = match payload.status {
        Some(value) => normalize_publication_status(&value)?,
        None => before_row.status.clone(),
    };

    let file = fetch_file_asset_for_update(&mut tx, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    ensure_file_can_link(&file, Some(&publication_id), is_public)?;

    let publication = sqlx::query_as::<_, PublicationRecord>(
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
    .bind(&category_id)
    .bind(&title)
    .bind(&description)
    .bind(effective_at)
    .bind(&file_id)
    .bind(is_public)
    .bind(sort_order)
    .bind(&status)
    .bind(&publication_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_constraint_error)?
    .ok_or(ApiError::BadRequest)?;

    if before_row.file_id != publication.file_id {
        detach_file_from_publication(&mut tx, &before_row.file_id, &publication.id).await?;
        attach_file_to_publication(&mut tx, &publication.file_id, &publication.id).await?;
    } else {
        attach_file_to_publication(&mut tx, &publication.file_id, &publication.id).await?;
    }

    let response = fetch_publication_in_tx(&mut tx, &publication_id)
        .await?
        .ok_or(ApiError::Internal)?;
    let response = Publication::from(response);

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        AuditAction::Update,
        "PUBLICATION",
        &response.id,
        Some(&before),
        Some(&response),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/api/v1/admin/publications/{publication_id}",
    tag = "publications",
    params(
        ("publication_id" = String, Path, description = "Publication ID")
    ),
    responses(
        (status = 204, description = "Publication deleted"),
        (status = 400, description = "Invalid publication ID"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn delete_publication(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(publication_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    ensure_web_update(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before = fetch_publication_in_tx(&mut tx, &publication_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    let before = Publication::from(before);

    let publication = fetch_publication_record_for_update(&mut tx, &publication_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let result = sqlx::query("delete from web.publications where id = $1")
        .bind(&publication_id)
        .execute(&mut *tx)
        .await
        .map_err(map_constraint_error)?;

    if result.rows_affected() == 0 {
        return Err(ApiError::BadRequest);
    }

    detach_file_from_publication(&mut tx, &publication.file_id, &publication.id).await?;

    record_audit_entry(
        &mut tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
        AuditAction::Delete,
        "PUBLICATION",
        &before.id,
        Some(&before),
        None::<&Publication>,
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_web_update(
    state: &AppState,
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
) -> Result<(), ApiError> {
    ensure_permission(
        state,
        current_user,
        current_service_account,
        PermissionKey::new(PermissionResource::Web, PermissionAction::Update),
    )
    .await
}

async fn fetch_publication_categories(
    pool: &sqlx::PgPool,
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

async fn fetch_publications(
    pool: &sqlx::PgPool,
    public_only: bool,
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
        order by c.sort_order asc, p.sort_order asc, p.effective_at desc, p.title asc
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
        order by c.sort_order asc, p.sort_order asc, p.effective_at desc, p.title asc
        "#
    };

    sqlx::query_as::<_, PublicationRow>(query)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

async fn fetch_publication(
    pool: &sqlx::PgPool,
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

async fn fetch_publication_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    publication_id: &str,
) -> Result<Option<PublicationRow>, ApiError> {
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
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn fetch_publication_record_for_update(
    tx: &mut Transaction<'_, Postgres>,
    publication_id: &str,
) -> Result<Option<PublicationRecord>, ApiError> {
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
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn fetch_publication_category_for_update(
    tx: &mut Transaction<'_, Postgres>,
    category_id: &str,
) -> Result<Option<PublicationCategory>, ApiError> {
    sqlx::query_as::<_, PublicationCategory>(
        r#"
        select id, key, name, description, sort_order, created_at, updated_at
        from web.publication_categories
        where id = $1
        for update
        "#,
    )
    .bind(category_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn ensure_category_exists(
    tx: &mut Transaction<'_, Postgres>,
    category_id: &str,
) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from web.publication_categories where id = $1)",
    )
    .bind(category_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    if exists {
        Ok(())
    } else {
        Err(ApiError::BadRequest)
    }
}

async fn fetch_file_asset_for_update(
    tx: &mut Transaction<'_, Postgres>,
    file_id: &str,
) -> Result<Option<FileAssetLinkRow>, ApiError> {
    sqlx::query_as::<_, FileAssetLinkRow>(
        r#"
        select is_public, domain_type, domain_id, deleted_at
        from media.file_assets
        where id = $1
        for update
        "#,
    )
    .bind(file_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

fn ensure_file_can_link(
    file: &FileAssetLinkRow,
    publication_id: Option<&str>,
    publication_is_public: bool,
) -> Result<(), ApiError> {
    if file.deleted_at.is_some() {
        return Err(ApiError::BadRequest);
    }

    if publication_is_public && !file.is_public {
        return Err(ApiError::BadRequest);
    }

    match (file.domain_type.as_deref(), file.domain_id.as_deref()) {
        (None, None) => Ok(()),
        (Some("publication"), Some(existing_id)) if Some(existing_id) == publication_id => Ok(()),
        _ => Err(ApiError::BadRequest),
    }
}

async fn attach_file_to_publication(
    tx: &mut Transaction<'_, Postgres>,
    file_id: &str,
    publication_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update media.file_assets
        set domain_type = 'publication', domain_id = $1
        where id = $2
        "#,
    )
    .bind(publication_id)
    .bind(file_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn detach_file_from_publication(
    tx: &mut Transaction<'_, Postgres>,
    file_id: &str,
    publication_id: &str,
) -> Result<(), ApiError> {
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
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn record_audit_entry<TBefore, TAfter>(
    tx: &mut Transaction<'_, Postgres>,
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
    action: AuditAction,
    resource_type: &str,
    resource_id: &str,
    before_state: Option<&TBefore>,
    after_state: Option<&TAfter>,
    headers: &HeaderMap,
) -> Result<(), ApiError>
where
    TBefore: Serialize,
    TAfter: Serialize,
{
    let actor =
        audit_repo::resolve_audit_actor(&mut **tx, current_user, current_service_account).await?;
    audit_repo::record_audit(
        &mut **tx,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.as_str().to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id.to_string()),
            scope_type: "web".to_string(),
            scope_key: Some(resource_id.to_string()),
            before_state: before_state
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            after_state: after_state
                .map(audit_repo::sanitized_snapshot)
                .transpose()?,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}

fn normalize_required_text(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(ApiError::BadRequest);
    }

    Ok(normalized.to_string())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(normalize_optional_text_value)
}

fn normalize_optional_text_value(raw: String) -> Option<String> {
    let normalized = raw.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn normalize_publication_status(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "draft" | "published" | "archived" => Ok(normalized),
        _ => Err(ApiError::BadRequest),
    }
}

fn normalize_category_key(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.starts_with('-') || normalized.ends_with('-') {
        return Err(ApiError::BadRequest);
    }

    if normalized
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        Ok(normalized)
    } else {
        Err(ApiError::BadRequest)
    }
}

fn normalize_id(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        Err(ApiError::BadRequest)
    } else {
        Ok(normalized.to_string())
    }
}

fn map_constraint_error(error: sqlx::Error) -> ApiError {
    match &error {
        sqlx::Error::Database(database_error) => match database_error.code().as_deref() {
            Some("23503") | Some("23505") | Some("23514") | Some("22P02") | Some("23502") => {
                ApiError::BadRequest
            }
            _ => ApiError::Internal,
        },
        _ => ApiError::Internal,
    }
}

#[derive(Debug, Clone, Copy)]
enum AuditAction {
    Create,
    Update,
    Delete,
}

impl AuditAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "CREATE",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
        }
    }
}

struct NewPublicationInput {
    category_id: String,
    title: String,
    description: Option<String>,
    effective_at: chrono::DateTime<chrono::Utc>,
    file_id: String,
    is_public: bool,
    sort_order: i32,
    status: String,
}
