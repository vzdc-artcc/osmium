use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    auth::{
        context::{CurrentServiceAccount, CurrentUser},
        permissions::{
            PublicationsCategoriesCreate, PublicationsCategoriesDelete, PublicationsCategoriesRead,
            PublicationsCategoriesUpdate, PublicationsItemsCreate, PublicationsItemsDelete,
            PublicationsItemsRead, PublicationsItemsUpdate,
        },
        require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{
        CreatePublicationCategoryRequest, CreatePublicationRequest, ListPublicationsQuery,
        PaginationMeta, PaginationQuery, Publication, PublicationCategory, PublicationListResponse,
        UpdatePublicationCategoryRequest, UpdatePublicationRequest,
    },
    repos::{audit as audit_repo, publications as publications_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

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
    time: ResponseTimeContext,
) -> Result<ApiJson<Vec<PublicationCategory>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let categories = publications_repo::fetch_publication_categories(pool).await?;
    Ok(ApiJson::new(categories, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/publications",
    tag = "publications",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List public publications", body = PublicationListResponse)
    )
)]
pub async fn list_publications(
    State(state): State<AppState>,
    Query(query): Query<ListPublicationsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<PublicationListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = publications_repo::count_publications(pool, true).await?;
    let rows =
        publications_repo::fetch_publications(pool, true, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        PublicationListResponse {
            items: rows.into_iter().map(Publication::from).collect(),
            pagination: meta,
        },
        time,
    ))
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
        (status = 404, description = "Publication not found")
    )
)]
pub async fn get_publication(
    State(state): State<AppState>,
    Path(publication_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Publication>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let publication = publications_repo::fetch_publication(pool, &publication_id, true)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(publication.into(), time))
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
    _permission: RequirePermission<PublicationsCategoriesRead>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Vec<PublicationCategory>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let categories = publications_repo::fetch_publication_categories(pool).await?;
    Ok(ApiJson::new(categories, time))
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
    _permission: RequirePermission<PublicationsCategoriesCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreatePublicationCategoryRequest>,
) -> Result<(StatusCode, ApiJson<PublicationCategory>), ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let key = normalize_category_key(&payload.key)?;
    let name = normalize_required_text(&payload.name)?;
    let description = normalize_optional_text(payload.description);
    let sort_order = payload.sort_order.unwrap_or(0);

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let category = publications_repo::insert_category(
        &mut *tx,
        &Uuid::new_v4().to_string(),
        &key,
        &name,
        description.as_deref(),
        sort_order,
    )
    .await?;

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
    Ok((StatusCode::CREATED, ApiJson::new(category, time)))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Publication category not found")
    )
)]
pub async fn update_publication_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<PublicationsCategoriesUpdate>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdatePublicationCategoryRequest>,
) -> Result<ApiJson<PublicationCategory>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before = publications_repo::fetch_publication_category_for_update(&mut *tx, &category_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let key = match payload.key {
        Some(value) => Some(normalize_category_key(&value)?),
        None => None,
    };
    let name = match payload.name {
        Some(value) => Some(normalize_required_text(&value)?),
        None => None,
    };
    let description = payload.description.and_then(normalize_optional_text_value);

    let category = publications_repo::update_category(
        &mut *tx,
        &category_id,
        key.as_deref(),
        name.as_deref(),
        description.as_deref(),
        payload.sort_order,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

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
    Ok(ApiJson::new(category, time))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Publication category not found")
    )
)]
pub async fn delete_publication_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<PublicationsCategoriesDelete>,
    Path(category_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before = publications_repo::fetch_publication_category_for_update(&mut *tx, &category_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let rows_affected = publications_repo::delete_category(&mut tx, &category_id).await?;
    if rows_affected == 0 {
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
    params(PaginationQuery),
    responses(
        (status = 200, description = "List publications for admin", body = PublicationListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn admin_list_publications(
    State(state): State<AppState>,
    _permission: RequirePermission<PublicationsItemsRead>,
    Query(query): Query<ListPublicationsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<PublicationListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(25, 200);
    let total = publications_repo::count_publications(pool, false).await?;
    let rows =
        publications_repo::fetch_publications(pool, false, pagination.page_size, pagination.offset)
            .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        PublicationListResponse {
            items: rows.into_iter().map(Publication::from).collect(),
            pagination: meta,
        },
        time,
    ))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Publication not found")
    )
)]
pub async fn admin_get_publication(
    State(state): State<AppState>,
    _permission: RequirePermission<PublicationsItemsRead>,
    Path(publication_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Publication>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let publication = publications_repo::fetch_publication(pool, &publication_id, false)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(publication.into(), time))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/publications",
    tag = "publications",
    request_body = CreatePublicationRequest,
    responses(
        (status = 201, description = "Publication created", body = Publication),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Referenced file not found")
    )
)]
pub async fn create_publication(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<PublicationsItemsCreate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<CreatePublicationRequest>,
) -> Result<(StatusCode, ApiJson<Publication>), ApiError> {
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
    publications_repo::ensure_category_exists(&mut *tx, &input.category_id).await?;
    let file = publications_repo::fetch_file_asset_for_update(&mut *tx, &input.file_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    ensure_file_can_link(&file, None, input.is_public)?;

    let publication = publications_repo::insert_publication(
        &mut *tx,
        &publication_id,
        &input.category_id,
        &input.title,
        input.description.as_deref(),
        input.effective_at,
        &input.file_id,
        input.is_public,
        input.sort_order,
        &input.status,
    )
    .await?;

    publications_repo::attach_file_to_publication(&mut *tx, &publication.file_id, &publication.id)
        .await?;
    let response = publications_repo::fetch_publication_in_tx(&mut *tx, &publication.id)
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
    Ok((StatusCode::CREATED, ApiJson::new(response, time)))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Publication not found")
    )
)]
pub async fn update_publication(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<PublicationsItemsUpdate>,
    Path(publication_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdatePublicationRequest>,
) -> Result<ApiJson<Publication>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before_row =
        publications_repo::fetch_publication_record_for_update(&mut *tx, &publication_id)
            .await?
            .ok_or(ApiError::NotFound)?;
    let before = publications_repo::fetch_publication_in_tx(&mut *tx, &publication_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let before = Publication::from(before);

    let category_id = match payload.category_id {
        Some(value) => normalize_id(&value)?,
        None => before_row.category_id.clone(),
    };
    publications_repo::ensure_category_exists(&mut *tx, &category_id).await?;

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

    let file = publications_repo::fetch_file_asset_for_update(&mut *tx, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    ensure_file_can_link(&file, Some(&publication_id), is_public)?;

    let publication = publications_repo::update_publication_row(
        &mut *tx,
        &publication_id,
        &category_id,
        &title,
        description.as_deref(),
        effective_at,
        &file_id,
        is_public,
        sort_order,
        &status,
    )
    .await?
    .ok_or(ApiError::NotFound)?;

    if before_row.file_id != publication.file_id {
        publications_repo::detach_file_from_publication(
            &mut *tx,
            &before_row.file_id,
            &publication.id,
        )
        .await?;
        publications_repo::attach_file_to_publication(
            &mut *tx,
            &publication.file_id,
            &publication.id,
        )
        .await?;
    } else {
        publications_repo::attach_file_to_publication(
            &mut *tx,
            &publication.file_id,
            &publication.id,
        )
        .await?;
    }

    let response = publications_repo::fetch_publication_in_tx(&mut *tx, &publication_id)
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
    Ok(ApiJson::new(response, time))
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
        (status = 401, description = "Not authorized"),
        (status = 404, description = "Publication not found")
    )
)]
pub async fn delete_publication(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<PublicationsItemsDelete>,
    Path(publication_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before = publications_repo::fetch_publication_in_tx(&mut *tx, &publication_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let before = Publication::from(before);

    let publication =
        publications_repo::fetch_publication_record_for_update(&mut *tx, &publication_id)
            .await?
            .ok_or(ApiError::NotFound)?;

    let rows_affected = publications_repo::delete_publication_row(&mut tx, &publication_id).await?;
    if rows_affected == 0 {
        return Err(ApiError::BadRequest);
    }

    publications_repo::detach_file_from_publication(
        &mut *tx,
        &publication.file_id,
        &publication.id,
    )
    .await?;

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

fn ensure_file_can_link(
    file: &publications_repo::FileAssetLinkRow,
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
