use std::path::PathBuf;

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, KeyInit},
};
use axum::{
    Json,
    body::Bytes,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{Permission, fetch_user_access},
        middleware::{CurrentUser, ensure_permission},
    },
    errors::ApiError,
    models::{FileAsset, ListFilesQuery, UpdateFileMetadataRequest, UploadFileQuery},
    state::AppState,
};

type HmacSha256 = Hmac<Sha256>;
const ENCRYPTION_MAGIC: &[u8] = b"OSMENC1";
const NONCE_LEN: usize = 12;

#[derive(sqlx::FromRow)]
struct FileAssetRow {
    id: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    etag: String,
    storage_key: String,
    is_public: bool,
    uploaded_by: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct SignedUrlQuery {
    expires_in: Option<i64>,
    never_expire: Option<bool>,
}

#[derive(Serialize)]
pub struct SignedUrlResponse {
    url: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CdnTokenQuery {
    expires: Option<i64>,
    sig: Option<String>,
}

impl From<FileAssetRow> for FileAsset {
    fn from(row: FileAssetRow) -> Self {
        Self {
            id: row.id,
            filename: row.filename,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            etag: row.etag,
            is_public: row.is_public,
            uploaded_by: row.uploaded_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn list_files(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<Vec<FileAsset>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ManageFiles).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows = sqlx::query_as::<_, FileAssetRow>(
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
            created_at,
            updated_at
        from file_assets
        order by created_at desc
        limit $1 offset $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(rows.into_iter().map(FileAsset::from).collect()))
}

pub async fn upload_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<UploadFileQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<FileAsset>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_can_upload(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    if body.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let max_size = max_upload_bytes();
    if body.len() as u64 > max_size {
        return Err(ApiError::BadRequest);
    }

    let file_id = Uuid::new_v4().to_string();
    let fallback_name = format!("{file_id}.bin");
    let filename = sanitize_filename(query.filename.as_deref().unwrap_or(&fallback_name))?;
    let content_type = normalize_content_type(headers.get(header::CONTENT_TYPE))?;
    let etag = sha256_hex(&body);
    let storage_key = storage_key_for_id(&file_id);

    write_blob(&storage_key, &body).await?;

    let now = chrono::Utc::now();
    let row = sqlx::query_as::<_, FileAssetRow>(
        r#"
        insert into file_assets (
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
        returning
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            created_at,
            updated_at
        "#,
    )
    .bind(&file_id)
    .bind(&filename)
    .bind(&content_type)
    .bind(body.len() as i64)
    .bind(&etag)
    .bind(&storage_key)
    .bind(query.public.unwrap_or(true))
    .bind(&user.id)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok((StatusCode::CREATED, Json(row.into())))
}

pub async fn get_file_metadata(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
) -> Result<Json<FileAsset>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    ensure_can_read_file(&state, current_user.as_ref(), &row).await?;

    Ok(Json(row.into()))
}

pub async fn download_file_content(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    ensure_can_read_file(&state, current_user.as_ref(), &row).await?;

    stream_file_response(&row, headers.get(header::RANGE)).await
}

pub async fn get_signed_download_url(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    Query(query): Query<SignedUrlQuery>,
) -> Result<Json<SignedUrlResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    ensure_can_read_file(&state, Some(user), &row).await?;

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

    let base_url = std::env::var("CDN_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let url = format!(
        "{}/cdn/{}?expires={}&sig={}",
        base_url.trim_end_matches('/'),
        row.id,
        expires,
        sig
    );

    Ok(Json(SignedUrlResponse { url, expires_at }))
}

pub async fn cdn_download_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    Query(query): Query<CdnTokenQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    let token_valid = token_is_valid(&row.id, query.expires, query.sig.as_deref())?;
    if !row.is_public && !token_valid {
        ensure_can_read_file(&state, current_user.as_ref(), &row).await?;
    }

    stream_file_response(&row, headers.get(header::RANGE)).await
}

pub async fn update_file_metadata(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    Json(payload): Json<UpdateFileMetadataRequest>,
) -> Result<Json<FileAsset>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let existing = fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    ensure_can_mutate_file(&state, user, &existing).await?;

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

    let row = sqlx::query_as::<_, FileAssetRow>(
        r#"
        update file_assets
        set filename = coalesce($1, filename),
            content_type = coalesce($2, content_type),
            is_public = coalesce($3, is_public),
            updated_at = now()
        where id = $4
        returning
            id,
            filename,
            content_type,
            size_bytes,
            etag,
            storage_key,
            is_public,
            uploaded_by,
            created_at,
            updated_at
        "#,
    )
    .bind(filename)
    .bind(content_type)
    .bind(payload.is_public)
    .bind(&file_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    Ok(Json(row.into()))
}

pub async fn replace_file_content(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
    Query(query): Query<UploadFileQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<FileAsset>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    if body.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let max_size = max_upload_bytes();
    if body.len() as u64 > max_size {
        return Err(ApiError::BadRequest);
    }

    let existing = fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    ensure_can_mutate_file(&state, user, &existing).await?;

    let filename = query
        .filename
        .as_deref()
        .map(sanitize_filename)
        .transpose()?
        .unwrap_or(existing.filename.clone());
    let content_type = normalize_content_type(headers.get(header::CONTENT_TYPE))?;
    let etag = sha256_hex(&body);

    write_blob(&existing.storage_key, &body).await?;

    let row = sqlx::query_as::<_, FileAssetRow>(
        r#"
        update file_assets
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
            created_at,
            updated_at
        "#,
    )
    .bind(filename)
    .bind(content_type)
    .bind(body.len() as i64)
    .bind(etag)
    .bind(&file_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(row.into()))
}

pub async fn delete_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(file_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let existing = fetch_file_row(pool, &file_id)
        .await?
        .ok_or(ApiError::BadRequest)?;
    ensure_can_mutate_file(&state, user, &existing).await?;

    let storage_key = sqlx::query_scalar::<_, String>(
        "delete from file_assets where id = $1 returning storage_key",
    )
    .bind(&file_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;

    let full_path = storage_root().join(storage_key);
    match tokio::fs::remove_file(full_path).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(ApiError::Internal),
    }

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
                HeaderValue::from_str(&format!("bytes */{}", total_len)).map_err(|_| ApiError::Internal)?,
            );
            return Ok((StatusCode::RANGE_NOT_SATISFIABLE, headers, Vec::<u8>::new()).into_response());
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

async fn fetch_file_row(pool: &sqlx::PgPool, file_id: &str) -> Result<Option<FileAssetRow>, ApiError> {
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
            created_at,
            updated_at
        from file_assets
        where id = $1
        "#,
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn ensure_can_upload(state: &AppState, user: &CurrentUser) -> Result<(), ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id, &user.role).await?;
    if permissions.contains(&Permission::ManageFiles) || permissions.contains(&Permission::UploadFiles)
    {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

async fn ensure_can_mutate_file(
    state: &AppState,
    user: &CurrentUser,
    row: &FileAssetRow,
) -> Result<(), ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id, &user.role).await?;

    if permissions.contains(&Permission::ManageFiles) {
        return Ok(());
    }

    if row.uploaded_by == user.id && permissions.contains(&Permission::UploadFiles) {
        return Ok(());
    }

    Err(ApiError::Unauthorized)
}

async fn ensure_can_read_file(
    state: &AppState,
    current_user: Option<&CurrentUser>,
    row: &FileAssetRow,
) -> Result<(), ApiError> {
    if row.is_public {
        return Ok(());
    }

    let user = current_user.ok_or(ApiError::Unauthorized)?;
    if user.id == row.uploaded_by {
        return Ok(());
    }

    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id, &user.role).await?;
    if permissions.contains(&Permission::ManageFiles) {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
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
    format!("{:x}", hasher.finalize())
}

fn sign_download_token(file_id: &str, expires: i64) -> Result<String, ApiError> {
    let secret = std::env::var("FILE_SIGNING_SECRET").map_err(|_| ApiError::ServiceUnavailable)?;
    if secret.trim().is_empty() {
        return Err(ApiError::ServiceUnavailable);
    }

    let payload = format!("{}:{}", file_id, expires);
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::Internal)?;
    mac.update(payload.as_bytes());
    let digest = mac.finalize().into_bytes();
    Ok(hex_encode(&digest))
}

fn token_is_valid(file_id: &str, expires: Option<i64>, sig: Option<&str>) -> Result<bool, ApiError> {
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
    getrandom(&mut nonce).map_err(|_| ApiError::Internal)?;

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
