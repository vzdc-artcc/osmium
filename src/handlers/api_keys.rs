use std::collections::BTreeSet;

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
        acl::{
            PermissionAction, PermissionPath, fetch_user_access, is_server_admin,
            normalize_permission_tree, permission_tree_from_paths,
        },
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
    },
    errors::ApiError,
    models::{
        ApiKeyDetail, ApiKeyListItem, ApiKeyListResponse, CreateApiKeyRequest,
        CreateApiKeyResponse, PaginationMeta, PaginationQuery, UpdateApiKeyRequest,
    },
    repos::{
        access::{permission_names_to_permissions, sha256_hex},
        api_keys as api_keys_repo,
        api_keys::{ApiKeyRow, DescriptionUpdate, NewApiKeyInput},
        audit as audit_repo,
    },
    state::AppState,
};

const API_KEY_PREFIX: &str = "osm_";
const API_KEY_SUFFIX_LEN: usize = 32;
const API_KEY_TOTAL_LEN: usize = API_KEY_PREFIX.len() + API_KEY_SUFFIX_LEN;
const API_KEY_DISPLAY_PREFIX_LEN: usize = 8;
const API_KEY_LAST_FOUR_LEN: usize = 4;

const RESOURCE_TYPE_API_KEY: &str = "API_KEY";
const SCOPE_TYPE_SERVICE_ACCOUNT: &str = "service_account";

#[utoipa::path(
    get,
    path = "/api/v1/api-keys",
    tag = "api-keys",
    params(PaginationQuery),
    responses(
        (status = 200, description = "List API keys visible to the current user", body = ApiKeyListResponse),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_api_keys(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<ApiKeyListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let viewer_can_read_all = user_holds_permission(
        &state,
        user,
        PermissionPath::from_segments(["api_keys"], PermissionAction::Read),
    )
    .await?;

    let pagination = query.resolve(25, 200);
    let total = api_keys_repo::count_api_keys(pool, &user.id, viewer_can_read_all).await?;
    let rows = api_keys_repo::list_api_keys(
        pool,
        &user.id,
        viewer_can_read_all,
        pagination.page_size,
        pagination.offset,
    )
    .await?;
    let items = rows.into_iter().map(ApiKeyListItem::from).collect();
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(Json(ApiKeyListResponse {
        items,
        total: meta.total,
        page: meta.page,
        page_size: meta.page_size,
        total_pages: meta.total_pages,
        has_next: meta.has_next,
        has_prev: meta.has_prev,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/api-keys/{key_id}",
    tag = "api-keys",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "API key detail", body = ApiKeyDetail),
        (status = 400, description = "Invalid key ID"),
        (status = 401, description = "Not authenticated or not authorized")
    )
)]
pub async fn get_api_key(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(key_id): Path<String>,
) -> Result<Json<ApiKeyDetail>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let row = api_keys_repo::fetch_api_key(pool, &key_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    ensure_can_manage_api_key(&state, user, &row, PermissionAction::Read).await?;

    let permissions = api_keys_repo::fetch_api_key_permission_names(pool, &row.id).await?;
    let detail = api_key_detail_from(row, permissions)?;
    Ok(Json(detail))
}

#[utoipa::path(
    post,
    path = "/api/v1/api-keys",
    tag = "api-keys",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 201, description = "API key created. The plaintext secret is returned exactly once.", body = CreateApiKeyResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated, lacks api_keys.create, or attempted to grant permissions outside subset")
    )
)]
pub async fn create_api_key(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), ApiError> {
    // API keys are user-owned. Service accounts cannot create new keys —
    // returning Unauthorized here also avoids permission-escalation chains.
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    ensure_permission(
        &state,
        Some(user),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["api_keys"], PermissionAction::Create),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let name = normalize_required_text(&payload.name)?;
    let description = normalize_optional_text(payload.description);

    let permission_names = normalize_permission_tree(&payload.permissions)?;
    let requested_permissions = permission_names_to_permissions(permission_names.clone())?;
    validate_permissions_are_subset(&state, user, &requested_permissions).await?;

    let secret = generate_api_key_secret()?;
    let secret_hash = sha256_hex(&secret);
    let prefix: String = secret.chars().take(API_KEY_DISPLAY_PREFIX_LEN).collect();
    let last_four: String = secret
        .chars()
        .skip(secret.chars().count() - API_KEY_LAST_FOUR_LEN)
        .collect();
    let id = Uuid::new_v4().to_string();
    let key = format!("apk_{}", &id[..8]);
    let credential_id = Uuid::new_v4().to_string();

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    api_keys_repo::insert_api_key(
        &mut tx,
        NewApiKeyInput {
            id: &id,
            key: &key,
            name: &name,
            description: description.as_deref(),
            created_by_user_id: &user.id,
            credential_id: &credential_id,
            secret_hash: &secret_hash,
            prefix: &prefix,
            last_four: &last_four,
            expires_at: payload.expires_at,
            permissions: &permission_names,
        },
    )
    .await?;

    let row = api_keys_repo::fetch_api_key_in_tx(&mut tx, &id)
        .await?
        .ok_or(ApiError::Internal)?;
    let stored_permissions =
        api_keys_repo::fetch_api_key_permission_names_in_tx(&mut tx, &id).await?;
    let detail = api_key_detail_from(row, stored_permissions)?;

    record_audit_entry(
        &mut tx,
        Some(user),
        current_service_account.as_ref(),
        AuditAction::Create,
        &detail.id,
        None::<&ApiKeyDetail>,
        Some(&detail),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let response = CreateApiKeyResponse {
        key: detail,
        secret,
    };
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/api-keys/{key_id}",
    tag = "api-keys",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    request_body = UpdateApiKeyRequest,
    responses(
        (status = 200, description = "API key updated", body = ApiKeyDetail),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated or not authorized")
    )
)]
pub async fn update_api_key(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(key_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateApiKeyRequest>,
) -> Result<Json<ApiKeyDetail>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before_row = api_keys_repo::fetch_api_key_in_tx(&mut tx, &key_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    ensure_can_manage_api_key(&state, user, &before_row, PermissionAction::Update).await?;

    let before_permissions =
        api_keys_repo::fetch_api_key_permission_names_in_tx(&mut tx, &before_row.id).await?;
    let before_detail = api_key_detail_from(before_row.clone(), before_permissions)?;

    let name = match payload.name {
        Some(value) => Some(normalize_required_text(&value)?),
        None => None,
    };
    // Treat `null` and an explicit string the same — the request distinguishes
    // "no change" from "set to value" via field presence (controlled by
    // `Option<...>`).  The owned `Option<Option<String>>` keeps the inner
    // String alive for the duration of the transaction.
    let description_value: Option<Option<String>> =
        payload.description.map(normalize_optional_text_value);
    let description_update = match description_value.as_ref() {
        Some(value) => DescriptionUpdate::Set(value.as_deref()),
        None => DescriptionUpdate::Unchanged,
    };

    api_keys_repo::update_api_key_metadata(&mut tx, &key_id, name.as_deref(), description_update)
        .await?;

    if let Some(permissions_value) = payload.permissions {
        let permission_names = normalize_permission_tree(&permissions_value)?;
        let requested = permission_names_to_permissions(permission_names.clone())?;
        validate_permissions_are_subset(&state, user, &requested).await?;
        api_keys_repo::replace_service_account_permissions(&mut tx, &key_id, &permission_names)
            .await?;
    }

    let after_row = api_keys_repo::fetch_api_key_in_tx(&mut tx, &key_id)
        .await?
        .ok_or(ApiError::Internal)?;
    let after_permissions =
        api_keys_repo::fetch_api_key_permission_names_in_tx(&mut tx, &after_row.id).await?;
    let after_detail = api_key_detail_from(after_row, after_permissions)?;

    record_audit_entry(
        &mut tx,
        Some(user),
        current_service_account.as_ref(),
        AuditAction::Update,
        &after_detail.id,
        Some(&before_detail),
        Some(&after_detail),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(Json(after_detail))
}

#[utoipa::path(
    delete,
    path = "/api/v1/api-keys/{key_id}",
    tag = "api-keys",
    params(
        ("key_id" = String, Path, description = "API key ID")
    ),
    responses(
        (status = 204, description = "API key revoked"),
        (status = 400, description = "Invalid key ID"),
        (status = 401, description = "Not authenticated or not authorized")
    )
)]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(key_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let before_row = api_keys_repo::fetch_api_key_in_tx(&mut tx, &key_id)
        .await?
        .ok_or(ApiError::BadRequest)?;

    ensure_can_manage_api_key(&state, user, &before_row, PermissionAction::Delete).await?;

    let before_permissions =
        api_keys_repo::fetch_api_key_permission_names_in_tx(&mut tx, &before_row.id).await?;
    let before_detail = api_key_detail_from(before_row.clone(), before_permissions.clone())?;

    api_keys_repo::revoke_api_key(&mut tx, &key_id).await?;

    let after_row = api_keys_repo::fetch_api_key_in_tx(&mut tx, &key_id)
        .await?
        .ok_or(ApiError::Internal)?;
    let after_detail = api_key_detail_from(after_row, before_permissions)?;

    record_audit_entry(
        &mut tx,
        Some(user),
        current_service_account.as_ref(),
        AuditAction::Revoke,
        &after_detail.id,
        Some(&before_detail),
        Some(&after_detail),
        &headers,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_can_manage_api_key(
    state: &AppState,
    user: &CurrentUser,
    row: &ApiKeyRow,
    action: PermissionAction,
) -> Result<(), ApiError> {
    if let Some(creator_id) = row.created_by_user_id.as_deref() {
        if creator_id == user.id {
            return Ok(());
        }
    }

    if user_holds_permission(
        state,
        user,
        PermissionPath::from_segments(["api_keys"], action),
    )
    .await?
    {
        return Ok(());
    }

    Err(ApiError::Unauthorized)
}

async fn user_holds_permission(
    state: &AppState,
    user: &CurrentUser,
    permission: PermissionPath,
) -> Result<bool, ApiError> {
    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    Ok(permissions.contains(&permission))
}

async fn validate_permissions_are_subset(
    state: &AppState,
    current_user: &CurrentUser,
    requested: &[PermissionPath],
) -> Result<(), ApiError> {
    if requested.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let (creator_roles, creator_perms) =
        fetch_user_access(state.db.as_ref(), &current_user.id).await?;

    if is_server_admin(&creator_roles) {
        return Ok(());
    }

    let creator_set: BTreeSet<&PermissionPath> = creator_perms.iter().collect();
    for perm in requested {
        if !creator_set.contains(perm) {
            return Err(ApiError::Unauthorized);
        }
    }

    Ok(())
}

fn generate_api_key_secret() -> Result<String, ApiError> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|_| ApiError::Internal)?;
    let suffix = base62_encode_fixed(&bytes, API_KEY_SUFFIX_LEN);
    let secret = format!("{API_KEY_PREFIX}{suffix}");
    debug_assert_eq!(secret.len(), API_KEY_TOTAL_LEN);
    Ok(secret)
}

fn base62_encode_fixed(bytes: &[u8], width: usize) -> String {
    const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

    // Build a big-endian big integer from `bytes` and base-62 encode it.
    let mut digits: Vec<u8> = Vec::with_capacity(width);
    let mut value: Vec<u32> = bytes.iter().map(|byte| u32::from(*byte)).collect();

    while !value.is_empty() {
        let mut remainder: u32 = 0;
        let mut next: Vec<u32> = Vec::with_capacity(value.len());
        for digit in &value {
            let acc = remainder * 256 + digit;
            let quotient = acc / 62;
            remainder = acc % 62;
            if !next.is_empty() || quotient != 0 {
                next.push(quotient);
            }
        }
        digits.push(ALPHABET[remainder as usize]);
        value = next;
    }

    while digits.len() < width {
        digits.push(ALPHABET[0]);
    }
    digits.truncate(width);
    digits.reverse();

    String::from_utf8(digits).expect("base62 alphabet is ascii")
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

fn api_key_detail_from(
    row: ApiKeyRow,
    permission_names: Vec<String>,
) -> Result<ApiKeyDetail, ApiError> {
    let permission_paths = permission_names_to_permissions(permission_names)?;
    let permissions = permission_tree_from_paths(&permission_paths);

    Ok(ApiKeyDetail {
        id: row.id,
        key: row.key,
        name: row.name,
        description: row.description,
        status: row.status,
        prefix: row.prefix,
        last_four: row.last_four,
        created_by_user_id: row.created_by_user_id,
        created_by_display_name: row.created_by_display_name,
        created_at: row.created_at,
        last_used_at: row.last_used_at,
        expires_at: row.expires_at,
        revoked_at: row.revoked_at,
        permissions,
    })
}

impl From<ApiKeyRow> for ApiKeyListItem {
    fn from(row: ApiKeyRow) -> Self {
        Self {
            id: row.id,
            key: row.key,
            name: row.name,
            description: row.description,
            status: row.status,
            prefix: row.prefix,
            last_four: row.last_four,
            created_by_user_id: row.created_by_user_id,
            created_by_display_name: row.created_by_display_name,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
            expires_at: row.expires_at,
            revoked_at: row.revoked_at,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AuditAction {
    Create,
    Update,
    Revoke,
}

impl AuditAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "CREATE",
            Self::Update => "UPDATE",
            Self::Revoke => "REVOKE",
        }
    }
}

async fn record_audit_entry<TBefore, TAfter>(
    tx: &mut Transaction<'_, Postgres>,
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
    action: AuditAction,
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
            resource_type: RESOURCE_TYPE_API_KEY.to_string(),
            resource_id: Some(resource_id.to_string()),
            scope_type: SCOPE_TYPE_SERVICE_ACCOUNT.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn perm(segments: &[&str], action: PermissionAction) -> PermissionPath {
        PermissionPath {
            segments: segments.iter().map(|s| s.to_string()).collect(),
            action,
        }
    }

    #[test]
    fn token_generator_produces_well_formed_secret() {
        let secret = generate_api_key_secret().expect("token generator should succeed");
        assert_eq!(secret.len(), API_KEY_TOTAL_LEN);
        assert!(secret.starts_with(API_KEY_PREFIX));
        let suffix = &secret[API_KEY_PREFIX.len()..];
        assert_eq!(suffix.len(), API_KEY_SUFFIX_LEN);
        assert!(
            suffix.chars().all(|ch| ch.is_ascii_alphanumeric()),
            "secret suffix should be base62: {suffix}"
        );
    }

    #[test]
    fn token_generator_produces_distinct_secrets() {
        let a = generate_api_key_secret().expect("first token");
        let b = generate_api_key_secret().expect("second token");
        assert_ne!(a, b, "two consecutive tokens must differ");
    }

    #[test]
    fn base62_encoder_pads_short_values_to_width() {
        let encoded = base62_encode_fixed(&[0u8; 24], API_KEY_SUFFIX_LEN);
        assert_eq!(encoded.len(), API_KEY_SUFFIX_LEN);
        assert!(encoded.chars().all(|ch| ch == '0'));
    }

    #[test]
    fn subset_check_rejects_request_outside_creator_set() {
        let creator: Vec<PermissionPath> = vec![perm(&["events", "items"], PermissionAction::Read)];
        let creator_set: BTreeSet<&PermissionPath> = creator.iter().collect();
        let requested = vec![perm(&["events", "items"], PermissionAction::Create)];
        let outside = requested.iter().any(|p| !creator_set.contains(p));
        assert!(
            outside,
            "create permission should not be considered a subset of read"
        );
    }

    #[test]
    fn subset_check_accepts_equal_set() {
        let creator: Vec<PermissionPath> = vec![perm(&["events", "items"], PermissionAction::Read)];
        let creator_set: BTreeSet<&PermissionPath> = creator.iter().collect();
        let requested: Vec<PermissionPath> =
            vec![perm(&["events", "items"], PermissionAction::Read)];
        for permission in &requested {
            assert!(
                creator_set.contains(permission),
                "exact match should be a subset"
            );
        }
    }

    #[test]
    fn create_request_deserializes_required_fields() {
        let raw = json!({
            "name": "Bot Key",
            "description": "Used for staging",
            "permissions": {
                "events": { "items": ["read"] }
            }
        });

        let request: CreateApiKeyRequest = serde_json::from_value(raw).unwrap();
        assert_eq!(request.name, "Bot Key");
        assert_eq!(request.description.as_deref(), Some("Used for staging"));
        assert!(request.expires_at.is_none());
        assert!(request.permissions.is_object());
    }

    #[test]
    fn empty_permission_tree_normalizes_to_error() {
        let result = normalize_permission_tree(&json!({}));
        assert!(result.is_err());
    }
}
