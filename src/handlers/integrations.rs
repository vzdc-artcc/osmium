use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
    },
    email::service::actor_from_context,
    errors::ApiError,
    repos::audit as audit_repo,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordConfigItem {
    pub id: String,
    pub name: String,
    pub guild_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordChannelItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub channel_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordRoleItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub role_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct DiscordCategoryItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub category_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct OutboundJobItem {
    pub id: String,
    pub job_type: String,
    pub subject_type: Option<String>,
    pub subject_id: Option<String>,
    pub status: String,
    pub attempt_count: i32,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub payload: Value,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscordLinkStateBody {
    pub linked: bool,
    pub external_id: Option<String>,
    pub auth_url: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordConfigRequest {
    pub name: String,
    pub guild_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordConfigRequest {
    pub name: Option<String>,
    pub guild_id: Option<Option<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordChannelRequest {
    pub discord_config_id: String,
    pub name: String,
    pub channel_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordChannelRequest {
    pub name: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordRoleRequest {
    pub discord_config_id: String,
    pub name: String,
    pub role_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordRoleRequest {
    pub name: Option<String>,
    pub role_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDiscordCategoryRequest {
    pub discord_config_id: String,
    pub name: String,
    pub category_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateDiscordCategoryRequest {
    pub name: Option<String>,
    pub category_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AnnouncementRequest {
    pub title: String,
    pub body_markdown: String,
    pub details_url: Option<String>,
    pub send_email: Option<bool>,
    pub send_discord: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EventPublishDiscordRequest {
    pub ping_users: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscordLinkStartRequest {
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscordLinkCompleteRequest {
    pub code: String,
    pub state: String,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DiscordUnlinkRequest {
    pub external_id: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct OutboundJobsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiscordConfigBundle {
    pub configs: Vec<DiscordConfigItem>,
    pub channels: Vec<DiscordChannelItem>,
    pub roles: Vec<DiscordRoleItem>,
    pub categories: Vec<DiscordCategoryItem>,
}

#[utoipa::path(get, path = "/api/v1/me/discord", tag = "integrations", responses((status = 200, description = "Current Discord link state", body = DiscordLinkStateBody), (status = 401, description = "Not authenticated")))]
pub async fn get_my_discord(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<DiscordLinkStateBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let external_id = sqlx::query_scalar::<_, String>(
        "select external_id from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'user_identity' and local_id = $1",
    ).bind(&user.id).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?;
    Ok(Json(DiscordLinkStateBody {
        linked: external_id.is_some(),
        external_id,
        auth_url: None,
    }))
}

#[utoipa::path(post, path = "/api/v1/me/discord/link/start", tag = "integrations", request_body = DiscordLinkStartRequest, responses((status = 200, description = "Discord link start response", body = DiscordLinkStateBody), (status = 401, description = "Not authenticated")))]
pub async fn start_discord_link(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<DiscordLinkStartRequest>,
) -> Result<Json<DiscordLinkStateBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let state_token = Uuid::new_v4().to_string();
    let redirect_uri = payload
        .redirect_uri
        .or_else(|| std::env::var("DISCORD_REDIRECT_URI").ok());
    let client_id = std::env::var("DISCORD_CLIENT_ID").ok();
    let auth_url = if let (Some(client_id), Some(redirect_uri)) = (client_id, redirect_uri.clone())
    {
        Some(format!(
            "https://discord.com/oauth2/authorize?client_id={client_id}&response_type=code&scope=identify&redirect_uri={redirect_uri}&state={state_token}"
        ))
    } else {
        None
    };
    sqlx::query(
        r#"
        insert into integration.external_sync_mappings (id, system_code, entity_type, local_id, external_id, metadata, created_at, updated_at)
        values ($1, 'discord', 'oauth_state', $2, $3, $4, now(), now())
        on conflict (system_code, entity_type, local_id) do update
        set external_id = excluded.external_id,
            metadata = excluded.metadata,
            updated_at = now()
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&state_token)
    .bind(&user.id)
    .bind(json!({
        "user_id": user.id,
        "cid": user.cid,
        "redirect_uri": redirect_uri,
        "created_at": Utc::now(),
        "expires_at": Utc::now() + Duration::hours(1)
    }))
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(DiscordLinkStateBody {
        linked: false,
        external_id: None,
        auth_url,
    }))
}

#[utoipa::path(post, path = "/api/v1/me/discord/unlink", tag = "integrations", request_body = DiscordUnlinkRequest, responses((status = 200, description = "Discord link removed", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn unlink_discord(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(_payload): Json<DiscordUnlinkRequest>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    sqlx::query("delete from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'user_identity' and local_id = $1")
        .bind(&user.id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(ApiMessageBody {
        message: "discord link removed".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/me/discord/link/complete", tag = "integrations", request_body = DiscordLinkCompleteRequest, responses((status = 200, description = "Discord identity linked", body = DiscordLinkStateBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn complete_discord_link(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<DiscordLinkCompleteRequest>,
) -> Result<Json<DiscordLinkStateBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    if payload.code.trim().is_empty() || payload.state.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let state_row = fetch_discord_oauth_state(pool, payload.state.trim()).await?;
    if state_row.external_id != user.id {
        return Err(ApiError::Unauthorized);
    }
    let expires_at = state_row
        .metadata
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .ok_or(ApiError::BadRequest)?;
    if expires_at < Utc::now() {
        return Err(ApiError::BadRequest);
    }

    let redirect_uri = payload
        .redirect_uri
        .or_else(|| {
            state_row
                .metadata
                .get("redirect_uri")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| std::env::var("DISCORD_REDIRECT_URI").ok())
        .ok_or(ApiError::ServiceUnavailable)?;
    let client_id = std::env::var("DISCORD_CLIENT_ID").map_err(|_| ApiError::ServiceUnavailable)?;
    let client_secret =
        std::env::var("DISCORD_CLIENT_SECRET").map_err(|_| ApiError::ServiceUnavailable)?;

    let discord_identity = exchange_discord_code(
        payload.code.trim(),
        &redirect_uri,
        &client_id,
        &client_secret,
    )
    .await?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let existing_owner = sqlx::query_scalar::<_, String>(
        "select local_id from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'user_identity' and external_id = $1",
    )
    .bind(&discord_identity.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    if existing_owner
        .as_deref()
        .is_some_and(|owner| owner != user.id)
    {
        return Err(ApiError::BadRequest);
    }

    sqlx::query(
        r#"
        insert into integration.external_sync_mappings (id, system_code, entity_type, local_id, external_id, metadata, created_at, updated_at)
        values ($1, 'discord', 'user_identity', $2, $3, $4, now(), now())
        on conflict (system_code, entity_type, local_id) do update
        set external_id = excluded.external_id,
            metadata = excluded.metadata,
            updated_at = now()
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&user.id)
    .bind(&discord_identity.id)
    .bind(json!({
        "username": discord_identity.username,
        "global_name": discord_identity.global_name,
        "linked_at": Utc::now(),
    }))
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        "delete from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'oauth_state' and local_id = $1",
    )
    .bind(payload.state.trim())
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(DiscordLinkStateBody {
        linked: true,
        external_id: Some(discord_identity.id),
        auth_url: None,
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/integrations/discord/configs", tag = "integrations", responses((status = 200, description = "Discord configuration bundle", body = DiscordConfigBundle), (status = 401, description = "Not authenticated")))]
pub async fn list_discord_configs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<DiscordConfigBundle>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let configs = sqlx::query_as::<_, DiscordConfigItem>("select id, name, guild_id, created_at, updated_at from integration.discord_configs order by name asc").fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    let channels = sqlx::query_as::<_, DiscordChannelItem>("select id, discord_config_id, name, channel_id, created_at from integration.discord_channels order by name asc").fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    let roles = sqlx::query_as::<_, DiscordRoleItem>("select id, discord_config_id, name, role_id, created_at from integration.discord_roles order by name asc").fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    let categories = sqlx::query_as::<_, DiscordCategoryItem>("select id, discord_config_id, name, category_id, created_at from integration.discord_categories order by name asc").fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    Ok(Json(DiscordConfigBundle {
        configs,
        channels,
        roles,
        categories,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/configs", tag = "integrations", request_body = CreateDiscordConfigRequest, responses((status = 201, description = "Discord config created", body = DiscordConfigItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_config(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateDiscordConfigRequest>,
) -> Result<(StatusCode, Json<DiscordConfigItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let item = sqlx::query_as::<_, DiscordConfigItem>(
        "insert into integration.discord_configs (id, name, guild_id, created_at, updated_at) values ($1, $2, $3, now(), now()) returning id, name, guild_id, created_at, updated_at",
    ).bind(Uuid::new_v4().to_string()).bind(payload.name.trim()).bind(payload.guild_id.as_deref()).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/configs/{config_id}", tag = "integrations", params(("config_id" = String, Path, description = "Discord config ID")), request_body = UpdateDiscordConfigRequest, responses((status = 200, description = "Updated Discord config", body = DiscordConfigItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_discord_config(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(config_id): Path<String>,
    Json(payload): Json<UpdateDiscordConfigRequest>,
) -> Result<Json<DiscordConfigItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = sqlx::query_as::<_, DiscordConfigItem>(
        r#"
        update integration.discord_configs
        set name = coalesce($2, name),
            guild_id = case when $3::bool then $4 else guild_id end,
            updated_at = now()
        where id = $1
        returning id, name, guild_id, created_at, updated_at
        "#,
    )
    .bind(&config_id)
    .bind(
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .bind(payload.guild_id.is_some())
    .bind(payload.guild_id.flatten())
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)?;
    Ok(Json(item))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/channels", tag = "integrations", request_body = CreateDiscordChannelRequest, responses((status = 201, description = "Discord channel created", body = DiscordChannelItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_channel(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateDiscordChannelRequest>,
) -> Result<(StatusCode, Json<DiscordChannelItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = sqlx::query_as::<_, DiscordChannelItem>(
        "insert into integration.discord_channels (id, discord_config_id, name, channel_id, created_at) values ($1, $2, $3, $4, now()) returning id, discord_config_id, name, channel_id, created_at",
    ).bind(Uuid::new_v4().to_string()).bind(&payload.discord_config_id).bind(payload.name.trim()).bind(payload.channel_id.trim()).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/channels/{channel_id}", tag = "integrations", params(("channel_id" = String, Path, description = "Discord channel ID")), request_body = UpdateDiscordChannelRequest, responses((status = 200, description = "Updated Discord channel", body = DiscordChannelItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_discord_channel(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(channel_id): Path<String>,
    Json(payload): Json<UpdateDiscordChannelRequest>,
) -> Result<Json<DiscordChannelItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = sqlx::query_as::<_, DiscordChannelItem>(
        "update integration.discord_channels set name = coalesce($2, name), channel_id = coalesce($3, channel_id) where id = $1 returning id, discord_config_id, name, channel_id, created_at",
    ).bind(&channel_id).bind(payload.name.as_deref().map(str::trim).filter(|v| !v.is_empty())).bind(payload.channel_id.as_deref().map(str::trim).filter(|v| !v.is_empty())).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)?;
    Ok(Json(item))
}

#[utoipa::path(delete, path = "/api/v1/admin/integrations/discord/channels/{channel_id}", tag = "integrations", params(("channel_id" = String, Path, description = "Discord channel ID")), responses((status = 200, description = "Deleted Discord channel", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn delete_discord_channel(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(channel_id): Path<String>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    sqlx::query("delete from integration.discord_channels where id = $1")
        .bind(&channel_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(ApiMessageBody {
        message: "discord channel deleted".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/roles", tag = "integrations", request_body = CreateDiscordRoleRequest, responses((status = 201, description = "Discord role created", body = DiscordRoleItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_role(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateDiscordRoleRequest>,
) -> Result<(StatusCode, Json<DiscordRoleItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = sqlx::query_as::<_, DiscordRoleItem>(
        "insert into integration.discord_roles (id, discord_config_id, name, role_id, created_at) values ($1, $2, $3, $4, now()) returning id, discord_config_id, name, role_id, created_at",
    ).bind(Uuid::new_v4().to_string()).bind(&payload.discord_config_id).bind(payload.name.trim()).bind(payload.role_id.trim()).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/roles/{role_id}", tag = "integrations", params(("role_id" = String, Path, description = "Discord role ID")), request_body = UpdateDiscordRoleRequest, responses((status = 200, description = "Updated Discord role", body = DiscordRoleItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_discord_role(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(role_id): Path<String>,
    Json(payload): Json<UpdateDiscordRoleRequest>,
) -> Result<Json<DiscordRoleItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = sqlx::query_as::<_, DiscordRoleItem>(
        "update integration.discord_roles set name = coalesce($2, name), role_id = coalesce($3, role_id) where id = $1 returning id, discord_config_id, name, role_id, created_at",
    ).bind(&role_id).bind(payload.name.as_deref().map(str::trim).filter(|v| !v.is_empty())).bind(payload.role_id.as_deref().map(str::trim).filter(|v| !v.is_empty())).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)?;
    Ok(Json(item))
}

#[utoipa::path(delete, path = "/api/v1/admin/integrations/discord/roles/{role_id}", tag = "integrations", params(("role_id" = String, Path, description = "Discord role ID")), responses((status = 200, description = "Deleted Discord role", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn delete_discord_role(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(role_id): Path<String>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    sqlx::query("delete from integration.discord_roles where id = $1")
        .bind(&role_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(ApiMessageBody {
        message: "discord role deleted".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/categories", tag = "integrations", request_body = CreateDiscordCategoryRequest, responses((status = 201, description = "Discord category created", body = DiscordCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateDiscordCategoryRequest>,
) -> Result<(StatusCode, Json<DiscordCategoryItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = sqlx::query_as::<_, DiscordCategoryItem>(
        "insert into integration.discord_categories (id, discord_config_id, name, category_id, created_at) values ($1, $2, $3, $4, now()) returning id, discord_config_id, name, category_id, created_at",
    ).bind(Uuid::new_v4().to_string()).bind(&payload.discord_config_id).bind(payload.name.trim()).bind(payload.category_id.trim()).fetch_one(pool).await.map_err(|_| ApiError::BadRequest)?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/categories/{category_id}", tag = "integrations", params(("category_id" = String, Path, description = "Discord category ID")), request_body = UpdateDiscordCategoryRequest, responses((status = 200, description = "Updated Discord category", body = DiscordCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn update_discord_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(category_id): Path<String>,
    Json(payload): Json<UpdateDiscordCategoryRequest>,
) -> Result<Json<DiscordCategoryItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = sqlx::query_as::<_, DiscordCategoryItem>(
        "update integration.discord_categories set name = coalesce($2, name), category_id = coalesce($3, category_id) where id = $1 returning id, discord_config_id, name, category_id, created_at",
    ).bind(&category_id).bind(payload.name.as_deref().map(str::trim).filter(|v| !v.is_empty())).bind(payload.category_id.as_deref().map(str::trim).filter(|v| !v.is_empty())).fetch_optional(pool).await.map_err(|_| ApiError::Internal)?.ok_or(ApiError::BadRequest)?;
    Ok(Json(item))
}

#[utoipa::path(delete, path = "/api/v1/admin/integrations/discord/categories/{category_id}", tag = "integrations", params(("category_id" = String, Path, description = "Discord category ID")), responses((status = 200, description = "Deleted Discord category", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn delete_discord_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(category_id): Path<String>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    sqlx::query("delete from integration.discord_categories where id = $1")
        .bind(&category_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(ApiMessageBody {
        message: "discord category deleted".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/notifications/announcements", tag = "integrations", request_body = AnnouncementRequest, responses((status = 200, description = "Announcement queued", body = ApiMessageBody), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn queue_announcement(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    headers: HeaderMap,
    Json(payload): Json<AnnouncementRequest>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    if payload.title.trim().is_empty() || payload.body_markdown.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let resolved_actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    if payload.send_email.unwrap_or(true) {
        let actor = actor_from_context(Some(user), None, resolved_actor.actor_id.clone(), "api");
        let _ = state
            .email
            .enqueue_audience_send(
                pool,
                actor,
                "announcements.generic".to_string(),
                json!({
                    "headline": payload.title,
                    "body_markdown": payload.body_markdown,
                    "cta_url": payload.details_url,
                }),
                crate::models::EmailAudienceRequest {
                    roles: Vec::new(),
                    artcc: Vec::new(),
                    rating: Vec::new(),
                    receive_event_notifications: Some(true),
                    active_only: Some(true),
                },
            )
            .await;
    }
    if payload.send_discord.unwrap_or(true) {
        enqueue_outbound_job(
            pool,
            "discord.announcement",
            Some("announcement"),
            None,
            json!({
                "title": payload.title,
                "body_markdown": payload.body_markdown,
                "details_url": payload.details_url,
                "requested_by_cid": user.cid
            }),
        )
        .await?;
    }
    record_audit(
        pool,
        user,
        &headers,
        "QUEUE",
        "NOTIFICATION_ANNOUNCEMENT",
        None,
        None,
        Some(json!({ "title": payload.title })),
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "announcement queued".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/events/{event_id}/publish/discord", tag = "integrations", params(("event_id" = String, Path, description = "Event ID")), request_body = EventPublishDiscordRequest, responses((status = 200, description = "Event Discord publish queued", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn queue_event_publish_discord(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<EventPublishDiscordRequest>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    enqueue_outbound_job(
        pool,
        "discord.event_positions_published",
        Some("event"),
        Some(&event_id),
        json!({
            "event_id": event_id,
            "ping_users": payload.ping_users.unwrap_or(false),
            "requested_by_cid": user.cid
        }),
    )
    .await?;
    record_audit(
        pool,
        user,
        &headers,
        "QUEUE",
        "EVENT_DISCORD_PUBLISH",
        Some(event_id.clone()),
        None,
        Some(json!({ "event_id": event_id })),
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "event publish queued".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/integrations/outbound-jobs", tag = "integrations", params(OutboundJobsQuery), responses((status = 200, description = "Outbound integration jobs", body = [OutboundJobItem]), (status = 401, description = "Not authenticated")))]
pub async fn list_outbound_jobs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<OutboundJobsQuery>,
) -> Result<Json<Vec<OutboundJobItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let rows = sqlx::query_as::<_, OutboundJobItem>(
        r#"
        select id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
        from integration.outbound_jobs
        where ($1::text is null or status = $1)
        order by created_at desc
        limit $2 offset $3
        "#,
    ).bind(query.status.as_deref()).bind(limit).bind(offset).fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    Ok(Json(rows))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/outbound-jobs/run", tag = "integrations", responses((status = 200, description = "Attempted outbound integration job deliveries", body = [OutboundJobItem]), (status = 401, description = "Not authenticated")))]
pub async fn run_outbound_jobs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<OutboundJobItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let mut rows = sqlx::query_as::<_, OutboundJobItem>(
        r#"
        select id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
        from integration.outbound_jobs
        where status in ('pending', 'retry')
          and (next_attempt_at is null or next_attempt_at <= now())
        order by created_at asc
        limit 20
        "#,
    ).fetch_all(pool).await.map_err(|_| ApiError::Internal)?;
    let client = Client::new();
    for row in &mut rows {
        let result = dispatch_outbound_job(&client, row).await;
        let (status, error, next_attempt_at, attempt_count) = match result {
            Ok(_) => ("delivered".to_string(), None, None, row.attempt_count + 1),
            Err(message) => {
                let next = Some(Utc::now() + Duration::minutes(5));
                (
                    "retry".to_string(),
                    Some(message),
                    next,
                    row.attempt_count + 1,
                )
            }
        };
        let updated = sqlx::query_as::<_, OutboundJobItem>(
            r#"
            update integration.outbound_jobs
            set status = $2,
                attempt_count = $3,
                last_attempt_at = now(),
                next_attempt_at = $4,
                error = $5,
                updated_at = now()
            where id = $1
            returning id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
            "#,
        )
        .bind(&row.id)
        .bind(&status)
        .bind(attempt_count)
        .bind(next_attempt_at)
        .bind(error)
        .fetch_one(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
        *row = updated;
    }
    Ok(Json(rows))
}

#[derive(Debug, sqlx::FromRow)]
struct DiscordOauthStateRow {
    external_id: String,
    metadata: Value,
}

#[derive(Debug, Deserialize)]
struct DiscordTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct DiscordUserIdentity {
    id: String,
    username: String,
    global_name: Option<String>,
}

async fn dispatch_outbound_job(client: &Client, job: &OutboundJobItem) -> Result<(), String> {
    let base_url = std::env::var("BOT_API_BASE_URL").ok();
    let api_key = std::env::var("BOT_API_SECRET_KEY").ok();
    let Some(base_url) = base_url else {
        return Err("missing BOT_API_BASE_URL".to_string());
    };
    let Some(api_key) = api_key else {
        return Err("missing BOT_API_SECRET_KEY".to_string());
    };

    let (path, body) = match job.job_type.as_str() {
        "discord.announcement" => ("/announcement", job.payload.clone()),
        "discord.event_positions_published" => ("/event_position_posting", job.payload.clone()),
        _ => return Err("unsupported job type".to_string()),
    };
    let response = client
        .post(format!("{base_url}{path}"))
        .header("Content-Type", "application/json")
        .header("X-API-Key", api_key)
        .json(&body)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "provider error".to_string());
        return Err(text);
    }
    Ok(())
}

async fn fetch_discord_oauth_state(
    pool: &sqlx::PgPool,
    state_token: &str,
) -> Result<DiscordOauthStateRow, ApiError> {
    sqlx::query_as::<_, DiscordOauthStateRow>(
        "select external_id, metadata from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'oauth_state' and local_id = $1",
    )
    .bind(state_token)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .ok_or(ApiError::BadRequest)
}

async fn exchange_discord_code(
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<DiscordUserIdentity, ApiError> {
    let client = Client::new();
    let token = client
        .post("https://discord.com/api/oauth2/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;
    if !token.status().is_success() {
        return Err(ApiError::BadRequest);
    }
    let token_body = token
        .json::<DiscordTokenResponse>()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    let identity = client
        .get("https://discord.com/api/users/@me")
        .bearer_auth(&token_body.access_token)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;
    if !identity.status().is_success() {
        return Err(ApiError::BadRequest);
    }
    identity
        .json::<DiscordUserIdentity>()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)
}

async fn enqueue_outbound_job(
    pool: &sqlx::PgPool,
    job_type: &str,
    subject_type: Option<&str>,
    subject_id: Option<&str>,
    payload: Value,
) -> Result<OutboundJobItem, ApiError> {
    sqlx::query_as::<_, OutboundJobItem>(
        r#"
        insert into integration.outbound_jobs (
            id, job_type, subject_type, subject_id, status, attempt_count, payload, created_at, updated_at
        )
        values ($1, $2, $3, $4, 'pending', 0, $5, now(), now())
        returning id, job_type, subject_type, subject_id, status, attempt_count, last_attempt_at, next_attempt_at, payload, error, created_at, updated_at
        "#,
    ).bind(Uuid::new_v4().to_string()).bind(job_type).bind(subject_type).bind(subject_id).bind(payload).fetch_one(pool).await.map_err(|_| ApiError::Internal)
}

async fn ensure_integrations_manage(state: &AppState, user: &CurrentUser) -> Result<(), ApiError> {
    ensure_permission(
        state,
        Some(user),
        None,
        PermissionPath::from_segments(["integrations", "stats"], PermissionAction::Update),
    )
    .await
}

async fn record_audit(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    headers: &HeaderMap,
    action: &str,
    resource_type: &str,
    resource_id: Option<String>,
    before_state: Option<Value>,
    after_state: Option<Value>,
) -> Result<(), ApiError> {
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            scope_type: "global".to_string(),
            scope_key: Some(user.cid.to_string()),
            before_state,
            after_state,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}
