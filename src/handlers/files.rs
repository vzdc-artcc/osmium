use std::path::PathBuf;

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, KeyInit as AeadKeyInit},
};
use axum::{
    Json,
    body::Bytes,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use hmac::digest::KeyInit as HmacKeyInit;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub use crate::repos::files::FileAssetRow;
use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath, fetch_access_catalog, fetch_user_access},
        context::CurrentUser,
        permissions::{
            FilesAssetsCreate, FilesAssetsDelete, FilesAssetsRead, FilesAuditRead,
            FilesContentCreate, FilesContentUpdate,
        },
        require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{
        CdnTokenQuery, FileAsset, FileAssetListResponse, FileAuditLogListResponse, FileAuditQuery,
        ListFilesQuery, PaginationMeta, PaginationQuery, SignedUrlQuery, SignedUrlResponse,
        UpdateFileMetadataRequest, UploadFileQuery,
    },
    repos::{audit as audit_repo, files as files_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

type HmacSha256 = Hmac<Sha256>;
const ENCRYPTION_MAGIC: &[u8] = b"OSMENC1";
const NONCE_LEN: usize = 12;

#[utoipa::path(
    get,
    path = "/api/v1/admin/files/audit",
    tag = "files",
    params(
        PaginationQuery,
        ("file_id" = Option<String>, Query, description = "Optional file ID filter")
    ),
    responses(
        (status = 200, description = "File audit log rows", body = FileAuditLogListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_file_audit_logs(
    State(state): State<AppState>,
    _permission: RequirePermission<FilesAuditRead>,
    Query(query): Query<FileAuditQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<FileAuditLogListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 250);
    let total = files_repo::count_audit_logs(pool, query.file_id.as_deref()).await?;
    let rows = files_repo::list_audit_logs(
        pool,
        query.file_id.as_deref(),
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        FileAuditLogListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/files",
    tag = "files",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List file assets", body = FileAssetListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_files(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FilesAssetsRead>,
    Query(query): Query<ListFilesQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<FileAssetListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    // RequirePermission<FilesAssetsRead> already enforced the read permission; this
    // second fetch is only to get the caller's roles for visibility filtering below.
    let (roles, _) = fetch_user_access(state.db.as_ref(), &user.id).await?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 200);
    let total = files_repo::count_visible_files(pool, &user.id, &roles).await?;
    let rows = files_repo::list_visible_files(
        pool,
        &user.id,
        &roles,
        pagination.page_size,
        pagination.offset,
    )
    .await?;

    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        FileAssetListResponse {
            items: rows.into_iter().map(FileAsset::from).collect(),
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/files",
    tag = "files",
    params(
        ("filename" = Option<String>, Query, description = "Upload filename"),
        ("public" = Option<bool>, Query, description = "Whether the file is public"),
        ("owner_cid" = Option<i64>, Query, description = "Optional owner CID"),
        ("viewer_cids" = Option<String>, Query, description = "Comma-separated viewer CIDs"),
        ("viewer_roles" = Option<String>, Query, description = "Comma-separated viewer roles")
    ),
    request_body(content = String, description = "Raw file bytes"),
    responses(
        (status = 201, description = "File uploaded", body = FileAsset),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "owner_cid does not reference an existing user")
    )
)]
pub async fn upload_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _asset_permission: RequirePermission<FilesAssetsCreate>,
    _content_permission: RequirePermission<FilesContentCreate>,
    Query(query): Query<UploadFileQuery>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    body: Bytes,
) -> Result<(StatusCode, ApiJson<FileAsset>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let ip_address = client_ip(&headers);

    if body.is_empty() {
        record_file_audit(
            pool,
            "upload",
            None,
            Some(&user.id),
            &ip_address,
            "bad_request",
            serde_json::json!({"reason": "empty_body"}),
        )
        .await;
        return Err(ApiError::BadRequest);
    }

    let max_size = max_upload_bytes();
    if body.len() as u64 > max_size {
        record_file_audit(
            pool,
            "upload",
            None,
            Some(&user.id),
            &ip_address,
            "bad_request",
            serde_json::json!({"reason": "max_size_exceeded", "max_size": max_size}),
        )
        .await;
        return Err(ApiError::BadRequest);
    }

    let owner_user_id = if let Some(owner_cid) = query.owner_cid {
        Some(
            files_repo::resolve_user_id_by_cid(pool, owner_cid)
                .await?
                .ok_or(ApiError::NotFound)?,
        )
    } else {
        None
    };
    let viewer_cids = parse_csv_i64(query.viewer_cids.as_deref())?;
    let allowed_user_ids = resolve_user_ids_by_cids(pool, &viewer_cids).await?;
    let viewer_roles = normalize_roles(
        parse_csv_strings(query.viewer_roles.as_deref())?,
        state.db.as_ref(),
    )
    .await?;

    let file_id = Uuid::new_v4().to_string();
    let fallback_name = format!("{file_id}.bin");
    let filename = sanitize_filename(query.filename.as_deref().unwrap_or(&fallback_name))?;
    let content_type = normalize_content_type(headers.get(header::CONTENT_TYPE))?;
    let etag = sha256_hex(&body);
    let storage_key = storage_key_for_id(&file_id);

    write_blob(&storage_key, &body).await?;

    let now = chrono::Utc::now();
    let row = files_repo::insert_file_asset(
        pool,
        &file_id,
        &filename,
        &content_type,
        body.len() as i64,
        &etag,
        &storage_key,
        query.public.unwrap_or(true),
        &user.id,
        owner_user_id.as_deref(),
        &viewer_roles,
        now,
    )
    .await?;

    for allowed_user_id in &allowed_user_ids {
        files_repo::insert_allowed_user(pool, &file_id, allowed_user_id).await?;
    }

    record_file_audit(
        pool,
        "upload",
        Some(&file_id),
        Some(&user.id),
        &ip_address,
        "success",
        serde_json::json!({"viewer_roles": viewer_roles, "viewer_cid_count": viewer_cids.len()}),
    )
    .await;

    let created_asset: FileAsset = row.into();
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "CREATE".to_string(),
            resource_type: "FILE".to_string(),
            resource_id: Some(file_id.clone()),
            scope_type: "file".to_string(),
            scope_key: Some(file_id.clone()),
            before_state: None,
            after_state: Some(audit_repo::sanitized_snapshot(&created_asset)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok((StatusCode::CREATED, ApiJson::new(created_asset, time)))
}

#[utoipa::path(
    get,
    path = "/api/v1/files/{file_id}",
    tag = "files",
    params(
        ("file_id" = String, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "File metadata", body = FileAsset),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "File not found")
    )
)]
pub async fn get_file_metadata(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<FileAsset>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = files_repo::fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    ensure_can_read_file_metadata(&state, current_user.as_ref(), &row).await?;

    Ok(ApiJson::new(row.into(), time))
}

#[utoipa::path(
    get,
    path = "/api/v1/files/{file_id}/content",
    tag = "files",
    params(
        ("file_id" = String, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "File content stream"),
        (status = 206, description = "Partial file content"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "File not found")
    )
)]
pub async fn download_file_content(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let ip_address = client_ip(&headers);

    let row = files_repo::fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    ensure_can_read_file_content(&state, current_user.as_ref(), &row).await?;
    let response = stream_file_response(&row, headers.get(header::RANGE)).await?;

    record_file_audit(
        pool,
        "download",
        Some(&row.id),
        current_user.as_ref().map(|value| value.id.as_str()),
        &ip_address,
        "success",
        serde_json::json!({"source": "api"}),
    )
    .await;

    Ok(response)
}

#[utoipa::path(
    get,
    path = "/api/v1/files/{file_id}/signed-url",
    tag = "files",
    params(
        ("file_id" = String, Path, description = "File ID"),
        ("expires_in" = Option<i64>, Query, description = "Expiration in seconds"),
        ("never_expire" = Option<bool>, Query, description = "Issue a practically non-expiring URL")
    ),
    responses(
        (status = 200, description = "Signed download URL", body = SignedUrlResponse),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "File not found")
    )
)]
pub async fn get_signed_download_url(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    Query(query): Query<SignedUrlQuery>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<SignedUrlResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let ip_address = client_ip(&headers);

    let row = files_repo::fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    ensure_can_read_file_content(&state, Some(user), &row).await?;

    let never_expire = query.never_expire.unwrap_or(false);
    let (expires, expires_at) = if never_expire {
        // 9999-12-31T23:59:59Z marker for non-expiring signed URLs.
        let permanent = chrono::DateTime::<chrono::Utc>::from_timestamp(253402300799, 0)
            .ok_or(ApiError::Internal)?;
        (0_i64, permanent)
    } else {
        let expires_in = query.expires_in.unwrap_or(900).clamp(60, 86400);
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);
        (expires_at.timestamp(), expires_at)
    };
    let sig = sign_download_token(&row.id, expires)?;

    let base_url =
        std::env::var("CDN_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let url = format!(
        "{}/cdn/{}?expires={}&sig={}",
        base_url.trim_end_matches('/'),
        row.id,
        expires,
        sig
    );

    record_file_audit(
        pool,
        "signed_url_issued",
        Some(&row.id),
        Some(&user.id),
        &ip_address,
        "success",
        serde_json::json!({"never_expire": never_expire}),
    )
    .await;

    Ok(ApiJson::new(SignedUrlResponse { url, expires_at }, time))
}

#[utoipa::path(
    get,
    path = "/cdn/{file_id}",
    tag = "files",
    params(
        ("file_id" = String, Path, description = "File ID"),
        ("expires" = Option<i64>, Query, description = "Signed URL expiry timestamp"),
        ("sig" = Option<String>, Query, description = "Signed URL signature")
    ),
    responses(
        (status = 200, description = "CDN file download"),
        (status = 206, description = "Partial CDN file download"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "File not found")
    )
)]
pub async fn cdn_download_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    Query(query): Query<CdnTokenQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let ip_address = client_ip(&headers);

    let row = files_repo::fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let token_valid = token_is_valid(&row.id, query.expires, query.sig.as_deref())?;
    if !row.is_public && !token_valid {
        ensure_can_read_file_content(&state, current_user.as_ref(), &row).await?;
    }

    let response = stream_file_response(&row, headers.get(header::RANGE)).await?;

    record_file_audit(
        pool,
        "download",
        Some(&row.id),
        current_user.as_ref().map(|value| value.id.as_str()),
        &ip_address,
        "success",
        serde_json::json!({"source": "cdn", "token_valid": token_valid}),
    )
    .await;

    Ok(response)
}

#[utoipa::path(
    patch,
    path = "/api/v1/files/{file_id}",
    tag = "files",
    params(
        ("file_id" = String, Path, description = "File ID")
    ),
    request_body = UpdateFileMetadataRequest,
    responses(
        (status = 200, description = "Updated file metadata", body = FileAsset),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "File not found")
    )
)]
pub async fn update_file_metadata(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateFileMetadataRequest>,
) -> Result<ApiJson<FileAsset>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let existing = files_repo::fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let existing_asset: FileAsset = existing.clone().into();
    let changing_access_policy = payload.is_public.is_some()
        || payload.owner_cid.is_some()
        || payload.viewer_cids.is_some()
        || payload.viewer_roles.is_some();

    // Not a RequirePermission<P> case: the permission actually required depends on
    // which fields the request body touches (plain metadata vs. access-control
    // fields), so the check can't be pinned to a single fixed PermissionPath.
    if changing_access_policy {
        ensure_can_update_file_policy(&state, user).await?;
    } else {
        ensure_can_update_file_metadata(&state, user).await?;
    }

    let filename = payload
        .filename
        .as_deref()
        .map(sanitize_filename)
        .transpose()?;
    let content_type = payload
        .content_type
        .as_deref()
        .map(normalize_content_type_str)
        .transpose()?;
    let owner_user_id = match payload.owner_cid {
        Some(cid) => Some(
            files_repo::resolve_user_id_by_cid(pool, cid)
                .await?
                .ok_or(ApiError::NotFound)?,
        ),
        None => None,
    };
    let viewer_roles = match payload.viewer_roles.as_ref() {
        Some(roles) => Some(normalize_roles(roles.clone(), state.db.as_ref()).await?),
        None => None,
    };
    let viewer_user_ids = match payload.viewer_cids.as_ref() {
        Some(cids) => Some(resolve_user_ids_by_cids(pool, cids).await?),
        None => None,
    };

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let row = files_repo::update_file_metadata(
        &mut *tx,
        &file_id,
        filename.as_deref(),
        content_type.as_deref(),
        payload.is_public,
        owner_user_id.as_deref(),
        viewer_roles.as_deref(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;

    if let Some(viewer_user_ids) = viewer_user_ids {
        files_repo::clear_allowed_users(&mut *tx, &file_id).await?;

        for viewer_user_id in viewer_user_ids {
            files_repo::insert_allowed_user(&mut *tx, &file_id, &viewer_user_id).await?;
        }
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let updated_asset: FileAsset = row.into();
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "FILE".to_string(),
            resource_id: Some(file_id.clone()),
            scope_type: "file".to_string(),
            scope_key: Some(file_id),
            before_state: Some(audit_repo::sanitized_snapshot(&existing_asset)?),
            after_state: Some(audit_repo::sanitized_snapshot(&updated_asset)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(ApiJson::new(updated_asset, time))
}

#[utoipa::path(
    put,
    path = "/api/v1/files/{file_id}/content",
    tag = "files",
    params(
        ("file_id" = String, Path, description = "File ID"),
        ("filename" = Option<String>, Query, description = "Replacement filename"),
        ("public" = Option<bool>, Query, description = "Ignored compatibility upload flag"),
        ("owner_cid" = Option<i64>, Query, description = "Ignored compatibility owner flag"),
        ("viewer_cids" = Option<String>, Query, description = "Ignored compatibility viewer CID flag"),
        ("viewer_roles" = Option<String>, Query, description = "Ignored compatibility viewer role flag")
    ),
    request_body(content = String, description = "Raw replacement file bytes"),
    responses(
        (status = 200, description = "Replaced file content", body = FileAsset),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "File not found")
    )
)]
pub async fn replace_file_content(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FilesContentUpdate>,
    Path(file_id): Path<String>,
    Query(query): Query<UploadFileQuery>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    body: Bytes,
) -> Result<ApiJson<FileAsset>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    if body.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let max_size = max_upload_bytes();
    if body.len() as u64 > max_size {
        return Err(ApiError::BadRequest);
    }

    let existing = files_repo::fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let existing_asset: FileAsset = existing.clone().into();

    let filename = query
        .filename
        .as_deref()
        .map(sanitize_filename)
        .transpose()?
        .unwrap_or(existing.filename.clone());
    let content_type = normalize_content_type(headers.get(header::CONTENT_TYPE))?;
    let etag = sha256_hex(&body);

    write_blob(&existing.storage_key, &body).await?;

    let row = files_repo::replace_file_content_row(
        pool,
        &file_id,
        &filename,
        &content_type,
        body.len() as i64,
        &etag,
    )
    .await?;

    let updated_asset: FileAsset = row.into();
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "FILE_CONTENT".to_string(),
            resource_id: Some(file_id.clone()),
            scope_type: "file".to_string(),
            scope_key: Some(file_id),
            before_state: Some(audit_repo::sanitized_snapshot(&existing_asset)?),
            after_state: Some(audit_repo::sanitized_snapshot(&updated_asset)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(ApiJson::new(updated_asset, time))
}

#[utoipa::path(
    delete,
    path = "/api/v1/files/{file_id}",
    tag = "files",
    params(
        ("file_id" = String, Path, description = "File ID")
    ),
    responses(
        (status = 204, description = "File deleted"),
        (status = 401, description = "Not authorized"),
        (status = 404, description = "File not found")
    )
)]
pub async fn delete_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<FilesAssetsDelete>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let ip_address = client_ip(&headers);

    let existing = files_repo::fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let existing_asset: FileAsset = existing.clone().into();

    let storage_key = files_repo::delete_file_asset(pool, &file_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let full_path = storage_root().join(storage_key);
    match tokio::fs::remove_file(full_path).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(ApiError::Internal),
    }

    record_file_audit(
        pool,
        "delete",
        Some(&file_id),
        Some(&user.id),
        &ip_address,
        "success",
        serde_json::json!({}),
    )
    .await;

    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "DELETE".to_string(),
            resource_type: "FILE".to_string(),
            resource_id: Some(file_id.clone()),
            scope_type: "file".to_string(),
            scope_key: Some(file_id),
            before_state: Some(audit_repo::sanitized_snapshot(&existing_asset)?),
            after_state: None,
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn stream_file_response(
    row: &FileAssetRow,
    range_header: Option<&HeaderValue>,
) -> Result<Response, ApiError> {
    let bytes = read_blob(&row.storage_key).await?;
    let total_len = bytes.len();

    let parsed = parse_range_header(range_header, total_len);
    let (status, body, content_range) = match parsed {
        Ok(Some((start, end))) => {
            let sliced = bytes[start..=end].to_vec();
            (
                StatusCode::PARTIAL_CONTENT,
                sliced,
                Some(format!("bytes {}-{}/{}", start, end, total_len)),
            )
        }
        Ok(None) => (StatusCode::OK, bytes, None),
        Err(()) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            headers.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{}", total_len))
                    .map_err(|_| ApiError::Internal)?,
            );
            return Ok(
                (StatusCode::RANGE_NOT_SATISFIABLE, headers, Vec::<u8>::new()).into_response(),
            );
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&row.content_type).map_err(|_| ApiError::Internal)?,
    );
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&row.etag).map_err(|_| ApiError::Internal)?,
    );
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    let safe_filename = row.filename.replace('"', "_");
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("inline; filename=\"{}\"", safe_filename))
            .map_err(|_| ApiError::Internal)?,
    );

    if let Some(content_range) = content_range {
        headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&content_range).map_err(|_| ApiError::Internal)?,
        );
    }

    Ok((status, headers, body).into_response())
}

fn parse_range_header(
    range_header: Option<&HeaderValue>,
    total_len: usize,
) -> Result<Option<(usize, usize)>, ()> {
    let Some(range_header) = range_header else {
        return Ok(None);
    };
    let Ok(raw) = range_header.to_str() else {
        return Err(());
    };
    if !raw.starts_with("bytes=") {
        return Err(());
    }

    let value = &raw[6..];
    if value.contains(',') {
        return Err(());
    }

    let mut parts = value.splitn(2, '-');
    let start = parts.next().ok_or(())?;
    let end = parts.next().ok_or(())?;

    if total_len == 0 {
        return Err(());
    }

    if start.is_empty() {
        let suffix_len = end.parse::<usize>().map_err(|_| ())?;
        if suffix_len == 0 {
            return Err(());
        }
        let clamped = suffix_len.min(total_len);
        let range_start = total_len - clamped;
        return Ok(Some((range_start, total_len - 1)));
    }

    let range_start = start.parse::<usize>().map_err(|_| ())?;
    let range_end = if end.is_empty() {
        total_len - 1
    } else {
        end.parse::<usize>().map_err(|_| ())?
    };

    if range_start >= total_len || range_start > range_end {
        return Err(());
    }

    Ok(Some((range_start, range_end.min(total_len - 1))))
}

async fn ensure_can_update_file_metadata(
    state: &AppState,
    user: &CurrentUser,
) -> Result<(), ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    if permissions.contains(&PermissionPath::from_segments(
        ["files", "assets"],
        PermissionAction::Update,
    )) {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

async fn ensure_can_update_file_policy(
    state: &AppState,
    user: &CurrentUser,
) -> Result<(), ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    if permissions.contains(&PermissionPath::from_segments(
        ["files", "assets", "policy"],
        PermissionAction::Update,
    )) {
        return Ok(());
    }

    Err(ApiError::Unauthorized)
}

// The following two checks are data-dependent (ownership/role/direct-grant on the
// specific file row), so they stay as explicit in-handler checks rather than
// RequirePermission<P> — the extractor only covers coarse-grained role/permission
// gates, not per-row authorization.

async fn ensure_can_read_file_metadata(
    state: &AppState,
    current_user: Option<&CurrentUser>,
    row: &FileAssetRow,
) -> Result<(), ApiError> {
    let user = current_user.ok_or(ApiError::Unauthorized)?;
    if user_can_access_file(
        state,
        user,
        row,
        PermissionPath::from_segments(["files", "assets"], PermissionAction::Read),
    )
    .await?
    {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

async fn ensure_can_read_file_content(
    state: &AppState,
    current_user: Option<&CurrentUser>,
    row: &FileAssetRow,
) -> Result<(), ApiError> {
    let user = current_user.ok_or(ApiError::Unauthorized)?;
    if user_can_access_file(
        state,
        user,
        row,
        PermissionPath::from_segments(["files", "content"], PermissionAction::Read),
    )
    .await?
    {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

async fn user_can_access_file(
    state: &AppState,
    user: &CurrentUser,
    row: &FileAssetRow,
    permission: PermissionPath,
) -> Result<bool, ApiError> {
    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    if !permissions.contains(&permission) {
        return Ok(false);
    }

    if row.is_public || user.id == row.uploaded_by || row.owner_user_id.as_deref() == Some(&user.id)
    {
        return Ok(true);
    }

    if roles
        .iter()
        .any(|role| row.viewer_roles.iter().any(|allowed| allowed == role))
    {
        return Ok(true);
    }

    let Some(pool) = state.db.as_ref() else {
        return Ok(false);
    };

    let has_direct_user_access =
        files_repo::count_direct_user_access(pool, &row.id, &user.id).await?;

    Ok(has_direct_user_access > 0)
}

async fn resolve_user_ids_by_cids(
    pool: &sqlx::PgPool,
    cids: &[i64],
) -> Result<Vec<String>, ApiError> {
    let rows = files_repo::resolve_user_ids_by_cids(pool, cids).await?;

    if rows.len() != cids.len() {
        return Err(ApiError::BadRequest);
    }

    Ok(rows.into_iter().map(|(_, id)| id).collect())
}

fn parse_csv_i64(raw: Option<&str>) -> Result<Vec<i64>, ApiError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    let mut values = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = trimmed.parse::<i64>().map_err(|_| ApiError::BadRequest)?;
        if !values.contains(&parsed) {
            values.push(parsed);
        }
    }

    Ok(values)
}

fn parse_csv_strings(raw: Option<&str>) -> Result<Vec<String>, ApiError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };

    let mut values = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !values.iter().any(|value| value == trimmed) {
            values.push(trimmed.to_string());
        }
    }

    Ok(values)
}

async fn normalize_roles(
    roles: Vec<String>,
    pool: Option<&sqlx::PgPool>,
) -> Result<Vec<String>, ApiError> {
    if roles.is_empty() {
        return Ok(Vec::new());
    }

    let mut normalized = Vec::new();
    for role in roles {
        let role = role.trim().to_ascii_uppercase();
        if role.is_empty() {
            continue;
        }
        if !normalized.iter().any(|value| value == &role) {
            normalized.push(role);
        }
    }

    let (catalog_roles, _) = fetch_access_catalog(pool).await?;
    for role in &normalized {
        if !catalog_roles
            .iter()
            .any(|catalog_role| catalog_role == role)
        {
            return Err(ApiError::BadRequest);
        }
    }

    Ok(normalized)
}

fn client_ip(headers: &HeaderMap) -> String {
    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        if let Some(first) = value.split(',').next() {
            let parsed = first.trim();
            if !parsed.is_empty() {
                return parsed.to_string();
            }
        }
    }

    if let Some(value) = headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
    {
        let parsed = value.trim();
        if !parsed.is_empty() {
            return parsed.to_string();
        }
    }

    "unknown".to_string()
}

async fn record_file_audit(
    pool: &sqlx::PgPool,
    action: &str,
    file_id: Option<&str>,
    actor_user_id: Option<&str>,
    ip_address: &str,
    outcome: &str,
    details: serde_json::Value,
) {
    let _ = files_repo::insert_audit_log(
        pool,
        &Uuid::new_v4().to_string(),
        action,
        file_id,
        actor_user_id,
        ip_address,
        outcome,
        details,
    )
    .await;
}

fn storage_root() -> PathBuf {
    let root = std::env::var("FILE_STORAGE_ROOT").unwrap_or_else(|_| "./storage/files".to_string());
    PathBuf::from(root)
}

fn storage_key_for_id(file_id: &str) -> String {
    let shard = &file_id[0..2];
    format!("{shard}/{file_id}.bin")
}

fn max_upload_bytes() -> u64 {
    std::env::var("FILE_MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(25 * 1024 * 1024)
}

fn sanitize_filename(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 255
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return Err(ApiError::BadRequest);
    }

    Ok(trimmed.to_string())
}

fn normalize_content_type(value: Option<&http::HeaderValue>) -> Result<String, ApiError> {
    let content_type = value
        .and_then(|header_value| header_value.to_str().ok())
        .unwrap_or("application/octet-stream");

    normalize_content_type_str(content_type)
}

fn normalize_content_type_str(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim();
    if normalized.is_empty() || normalized.len() > 255 || normalized.contains('\n') {
        return Err(ApiError::BadRequest);
    }

    Ok(normalized.to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(hasher.finalize().as_ref())
}

fn sign_download_token(file_id: &str, expires: i64) -> Result<String, ApiError> {
    let secret = std::env::var("FILE_SIGNING_SECRET").map_err(|_| ApiError::ServiceUnavailable)?;
    if secret.trim().is_empty() {
        return Err(ApiError::ServiceUnavailable);
    }

    let payload = format!("{}:{}", file_id, expires);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ApiError::Internal)?;
    mac.update(payload.as_bytes());
    let digest = mac.finalize().into_bytes();
    Ok(hex_encode(&digest))
}

fn token_is_valid(
    file_id: &str,
    expires: Option<i64>,
    sig: Option<&str>,
) -> Result<bool, ApiError> {
    let Some(expires) = expires else {
        return Ok(false);
    };
    let Some(sig) = sig else {
        return Ok(false);
    };
    if expires != 0 && expires < chrono::Utc::now().timestamp() {
        return Ok(false);
    }

    let expected = sign_download_token(file_id, expires)?;
    Ok(constant_time_eq(sig.trim(), &expected))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut mismatch = 0u8;
    for (left, right) in a.as_bytes().iter().zip(b.as_bytes().iter()) {
        mismatch |= left ^ right;
    }

    mismatch == 0
}

async fn write_blob(storage_key: &str, body: &[u8]) -> Result<(), ApiError> {
    let full_path = storage_root().join(storage_key);
    let parent = full_path.parent().ok_or(ApiError::Internal)?;
    let payload = maybe_encrypt_blob(body)?;

    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|_| ApiError::Internal)?;

    let temp_path = full_path.with_extension("tmp");
    tokio::fs::write(&temp_path, &payload)
        .await
        .map_err(|_| ApiError::Internal)?;

    tokio::fs::rename(&temp_path, &full_path)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn read_blob(storage_key: &str) -> Result<Vec<u8>, ApiError> {
    let full_path = storage_root().join(storage_key);
    let bytes = tokio::fs::read(full_path)
        .await
        .map_err(|_| ApiError::BadRequest)?;

    maybe_decrypt_blob(&bytes)
}

fn maybe_encrypt_blob(plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
    let Some(cipher) = encryption_cipher()? else {
        return Ok(plaintext.to_vec());
    };

    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&Uuid::new_v4().into_bytes()[..NONCE_LEN]);

    let encrypted = cipher
        .encrypt((&nonce).into(), plaintext)
        .map_err(|_| ApiError::Internal)?;

    let mut out = Vec::with_capacity(ENCRYPTION_MAGIC.len() + NONCE_LEN + encrypted.len());
    out.extend_from_slice(ENCRYPTION_MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&encrypted);
    Ok(out)
}

fn maybe_decrypt_blob(bytes: &[u8]) -> Result<Vec<u8>, ApiError> {
    if !bytes.starts_with(ENCRYPTION_MAGIC) {
        // Backward compatibility for legacy plaintext blobs.
        return Ok(bytes.to_vec());
    }

    if bytes.len() < ENCRYPTION_MAGIC.len() + NONCE_LEN {
        return Err(ApiError::Internal);
    }

    let cipher = encryption_cipher()?.ok_or(ApiError::ServiceUnavailable)?;
    let nonce_start = ENCRYPTION_MAGIC.len();
    let nonce_end = nonce_start + NONCE_LEN;
    let nonce = &bytes[nonce_start..nonce_end];
    let ciphertext = &bytes[nonce_end..];

    cipher
        .decrypt(nonce.into(), ciphertext)
        .map_err(|_| ApiError::Internal)
}

fn encryption_cipher() -> Result<Option<Aes256Gcm>, ApiError> {
    let Some(key) = encryption_key_bytes()? else {
        return Ok(None);
    };

    Ok(Some(Aes256Gcm::new((&key).into())))
}

fn encryption_key_bytes() -> Result<Option<[u8; 32]>, ApiError> {
    let raw = match std::env::var("FILE_ENCRYPTION_KEY_HEX") {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let normalized = raw.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    if normalized.len() != 64 {
        return Err(ApiError::ServiceUnavailable);
    }

    let mut key = [0u8; 32];
    for (index, chunk) in normalized.as_bytes().chunks(2).enumerate() {
        key[index] = parse_hex_pair(chunk[0], chunk[1]).ok_or(ApiError::ServiceUnavailable)?;
    }

    Ok(Some(key))
}

fn parse_hex_pair(high: u8, low: u8) -> Option<u8> {
    let high = hex_nibble(high)?;
    let low = hex_nibble(low)?;
    Some((high << 4) | low)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
