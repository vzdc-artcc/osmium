use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Redirect,
};
use chrono::{Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::CurrentUser,
        middleware::ensure_permission,
        permissions::{AuthProfileRead, IntegrationsStatsUpdate},
        require_permission::RequirePermission,
    },
    email::service::actor_from_context,
    errors::ApiError,
    models::{
        AnnouncementRequest, BotFeatureFlag, BotFeatureFlagsResponse, CreateDiscordCategoryRequest,
        CreateDiscordChannelRequest, CreateDiscordConfigRequest, CreateDiscordRoleRequest,
        CreateDiscordScheduledEventRequest, DiscordCategoryItem, DiscordChannelItem,
        DiscordConfigBundle, DiscordConfigItem, DiscordLinkCallbackQuery, DiscordLinkStartRequest,
        DiscordLinkStateBody, DiscordRoleItem, DiscordUnlinkRequest, DiscoveredGuildListResponse,
        EventPublishDiscordRequest, GuildDiscoveryResponse, OutboundJobItem,
        OutboundJobListResponse, OutboundJobsQuery, PaginationMeta, PaginationQuery,
        UpdateBotFeatureFlagsRequest, UpdateDiscordCategoryRequest, UpdateDiscordChannelRequest,
        UpdateDiscordConfigRequest, UpdateDiscordRoleRequest,
    },
    repos::{audit as audit_repo, integrations as integrations_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiMessageBody {
    pub message: String,
}

#[utoipa::path(get, path = "/api/v1/me/discord", tag = "integrations", responses((status = 200, description = "Current Discord link state", body = DiscordLinkStateBody), (status = 401, description = "Not authenticated")))]
pub async fn get_my_discord(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
) -> Result<Json<DiscordLinkStateBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let external_id = integrations_repo::find_discord_link_by_user(pool, &user.id).await?;
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
    _permission: RequirePermission<AuthProfileRead>,
    Json(payload): Json<DiscordLinkStartRequest>,
) -> Result<Json<DiscordLinkStateBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let state_token = Uuid::new_v4().to_string();
    // Discord redirects to osmium's own callback, which performs the token
    // exchange server-side and then 302s the browser back to `return_url`. The
    // redirect_uri here is osmium's, not the website's.
    // The secret is only used at the callback, but without it every flow fails
    // there, so it gates `auth_url` too: clients show "not configured" up front.
    let redirect_uri = discord_env("DISCORD_REDIRECT_URI");
    let client_id = discord_env("DISCORD_CLIENT_ID");
    let client_secret = discord_env("DISCORD_CLIENT_SECRET");
    let auth_url = if let (Some(client_id), Some(redirect_uri), Some(_)) =
        (client_id, redirect_uri.clone(), client_secret)
    {
        // redirect_uri must be percent-encoded as a query value (it contains `:`
        // and `/`, and in prod may carry a query string); Discord compares it
        // byte-for-byte at token exchange.
        Some(format!(
            "https://discord.com/oauth2/authorize?client_id={client_id}&response_type=code&scope=identify&redirect_uri={}&state={state_token}",
            urlencoding::encode(&redirect_uri)
        ))
    } else {
        None
    };
    integrations_repo::upsert_discord_oauth_state(
        pool,
        &Uuid::new_v4().to_string(),
        &state_token,
        &user.id,
        json!({
            "user_id": user.id,
            "cid": user.cid,
            "redirect_uri": redirect_uri,
            "return_url": payload.return_url,
            "created_at": Utc::now(),
            "expires_at": Utc::now() + Duration::hours(1)
        }),
    )
    .await?;

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
    _permission: RequirePermission<AuthProfileRead>,
    Json(_payload): Json<DiscordUnlinkRequest>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    integrations_repo::delete_discord_link(pool, &user.id).await?;
    Ok(Json(ApiMessageBody {
        message: "discord link removed".to_string(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/me/discord/link/callback",
    tag = "integrations",
    params(DiscordLinkCallbackQuery),
    responses((status = 303, description = "Redirects back to the website with the link outcome"))
)]
pub async fn discord_link_callback(
    State(state): State<AppState>,
    Query(query): Query<DiscordLinkCallbackQuery>,
) -> Redirect {
    // Public endpoint: Discord redirects the browser here after authorization. The
    // acting user is identified from the single-use `state` token (bound to a user
    // at `start`), not the session cookie, so the flow survives the cross-site hop
    // regardless of cookie SameSite behavior. Every path resolves to a website URL
    // so the browser always lands somewhere with a readable outcome.
    Redirect::to(&run_discord_link_callback(&state, query).await)
}

async fn run_discord_link_callback(state: &AppState, query: DiscordLinkCallbackQuery) -> String {
    let fail = |return_url: Option<String>, reason: &str| {
        append_query(
            &resolve_return_url(return_url),
            &[("discord_error", reason)],
        )
    };

    let Some(pool) = state.db.as_ref() else {
        return fail(None, "server_error");
    };
    let Some(state_token) = query
        .state
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    else {
        return fail(None, "missing_state");
    };

    let state_row = match integrations_repo::fetch_discord_oauth_state(pool, state_token).await {
        Ok(Some(row)) => row,
        Ok(None) => return fail(None, "invalid_state"),
        Err(_) => return fail(None, "server_error"),
    };
    let return_url = state_row
        .metadata
        .get("return_url")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Discord bounced back with an explicit error (e.g. the user pressed Cancel).
    if let Some(err) = query
        .error
        .as_deref()
        .map(str::trim)
        .filter(|err| !err.is_empty())
    {
        return fail(return_url, err);
    }
    let Some(code) = query
        .code
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())
    else {
        return fail(return_url, "missing_code");
    };

    let fresh = state_row
        .metadata
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .is_some_and(|expires_at| expires_at >= Utc::now());
    if !fresh {
        return fail(return_url, "expired");
    }

    let redirect_uri = state_row
        .metadata
        .get("redirect_uri")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| discord_env("DISCORD_REDIRECT_URI"));
    let (Some(redirect_uri), Some(client_id), Some(client_secret)) = (
        redirect_uri,
        discord_env("DISCORD_CLIENT_ID"),
        discord_env("DISCORD_CLIENT_SECRET"),
    ) else {
        return fail(return_url, "server_error");
    };

    let identity =
        match exchange_discord_code(code, &redirect_uri, &client_id, &client_secret).await {
            Ok(identity) => identity,
            Err(_) => return fail(return_url, "link_failed"),
        };

    let user_id = state_row.external_id.clone();
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return fail(return_url, "server_error"),
    };
    match integrations_repo::find_discord_identity_owner(&mut *tx, &identity.id).await {
        Ok(Some(owner)) if owner != user_id => return fail(return_url, "already_linked"),
        Ok(_) => {}
        Err(_) => return fail(return_url, "server_error"),
    }
    if integrations_repo::insert_discord_user_identity(
        &mut *tx,
        &Uuid::new_v4().to_string(),
        &user_id,
        &identity.id,
        json!({
            "username": identity.username,
            "global_name": identity.global_name,
            "linked_at": Utc::now(),
        }),
    )
    .await
    .is_err()
    {
        return fail(return_url, "server_error");
    }
    if integrations_repo::delete_discord_oauth_state(&mut *tx, state_token)
        .await
        .is_err()
    {
        return fail(return_url, "server_error");
    }
    if tx.commit().await.is_err() {
        return fail(return_url, "server_error");
    }

    let display_name = identity.global_name.unwrap_or(identity.username);
    append_query(
        &resolve_return_url(return_url),
        &[("discord_linked", "1"), ("discord_username", &display_name)],
    )
}

/// Website origins osmium is willing to 302 back to after the Discord round-trip.
/// Reuses the CORS allowlist so the callback can never be turned into an open
/// redirector.
fn allowed_website_origins() -> Vec<String> {
    std::env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(|origin| origin.trim().trim_end_matches('/').to_string())
        .filter(|origin| !origin.is_empty())
        .collect()
}

/// Extracts the `scheme://authority` origin from a URL string, or `None` if it is
/// not shaped like an absolute URL.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}

/// Resolves the base website URL (no query string) to send the browser back to.
/// Honors the caller-supplied `return_url` only when its origin is allowlisted;
/// otherwise falls back to the first allowed origin + `/profile/overview`.
fn resolve_return_url(candidate: Option<String>) -> String {
    let allowed = allowed_website_origins();
    if let Some(url) = candidate
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        if let Some(origin) = origin_of(url) {
            if allowed
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&origin))
            {
                return url.trim_end_matches('/').to_string();
            }
        }
    }
    let base = allowed
        .into_iter()
        .next()
        .unwrap_or_else(|| "http://localhost:3000".to_string());
    format!("{base}/profile/overview")
}

/// Appends query params to a URL that may or may not already carry a query string.
fn append_query(base: &str, params: &[(&str, &str)]) -> String {
    let mut url = base.to_string();
    let mut separator = if base.contains('?') { '&' } else { '?' };
    for (key, value) in params {
        url.push(separator);
        url.push_str(key);
        url.push('=');
        url.push_str(&urlencoding::encode(value));
        separator = '&';
    }
    url
}

#[utoipa::path(get, path = "/api/v1/admin/integrations/discord/configs", tag = "integrations", responses((status = 200, description = "Discord configuration bundle", body = DiscordConfigBundle), (status = 401, description = "Not authenticated")))]
pub async fn list_discord_configs(
    State(state): State<AppState>,
    _permission: RequirePermission<IntegrationsStatsUpdate>,
    time: ResponseTimeContext,
) -> Result<ApiJson<DiscordConfigBundle>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let configs = integrations_repo::list_discord_configs(pool).await?;
    let channels = integrations_repo::list_discord_channels(pool).await?;
    let roles = integrations_repo::list_discord_roles(pool).await?;
    let categories = integrations_repo::list_discord_categories(pool).await?;
    Ok(ApiJson::new(
        DiscordConfigBundle {
            configs,
            channels,
            roles,
            categories,
        },
        time,
    ))
}

#[utoipa::path(get, path = "/api/v1/admin/integrations/discord/features", tag = "integrations", responses((status = 200, description = "Discord bot feature toggles", body = BotFeatureFlagsResponse), (status = 401, description = "Not authenticated")))]
pub async fn list_bot_features(
    State(state): State<AppState>,
    _permission: RequirePermission<IntegrationsStatsUpdate>,
) -> Result<Json<BotFeatureFlagsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let stored = integrations_repo::get_bot_feature_flags(pool).await?;
    let features = crate::models::integrations::BOT_FEATURES
        .iter()
        .map(|(key, label)| BotFeatureFlag {
            key: (*key).to_string(),
            label: (*label).to_string(),
            // Features default to enabled when there is no stored override.
            enabled: stored.get(*key).copied().unwrap_or(true),
        })
        .collect();
    Ok(Json(BotFeatureFlagsResponse { features }))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/features", tag = "integrations", request_body = UpdateBotFeatureFlagsRequest, responses((status = 200, description = "Updated Discord bot feature toggles", body = BotFeatureFlagsResponse), (status = 401, description = "Not authenticated")))]
pub async fn update_bot_features(
    State(state): State<AppState>,
    _permission: RequirePermission<IntegrationsStatsUpdate>,
    Json(payload): Json<UpdateBotFeatureFlagsRequest>,
) -> Result<Json<BotFeatureFlagsResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    // Only persist known feature keys.
    let flags: Vec<(String, bool)> = payload
        .features
        .into_iter()
        .filter(|(key, _)| {
            crate::models::integrations::BOT_FEATURES
                .iter()
                .any(|(known, _)| known == key)
        })
        .collect();
    if !flags.is_empty() {
        integrations_repo::upsert_bot_feature_flags(pool, &flags).await?;
    }
    let stored = integrations_repo::get_bot_feature_flags(pool).await?;
    let features = crate::models::integrations::BOT_FEATURES
        .iter()
        .map(|(key, label)| BotFeatureFlag {
            key: (*key).to_string(),
            label: (*label).to_string(),
            enabled: stored.get(*key).copied().unwrap_or(true),
        })
        .collect();
    Ok(Json(BotFeatureFlagsResponse { features }))
}

#[utoipa::path(get, path = "/api/v1/admin/integrations/discord/guilds", tag = "integrations", responses((status = 200, description = "Guilds the Discord bot is a member of", body = DiscoveredGuildListResponse), (status = 401, description = "Not authenticated"), (status = 503, description = "Discord bot unavailable")))]
pub async fn list_discord_guilds(
    _permission: RequirePermission<IntegrationsStatsUpdate>,
) -> Result<Json<DiscoveredGuildListResponse>, ApiError> {
    let response = bot_api_get::<DiscoveredGuildListResponse>("/guilds").await?;
    Ok(Json(response))
}

#[utoipa::path(get, path = "/api/v1/admin/integrations/discord/guilds/{guild_id}/discovery", tag = "integrations", params(("guild_id" = String, Path, description = "Discord guild id")), responses((status = 200, description = "Live channels, categories, and roles for the guild", body = GuildDiscoveryResponse), (status = 401, description = "Not authenticated"), (status = 503, description = "Discord bot unavailable")))]
pub async fn discover_discord_guild(
    _permission: RequirePermission<IntegrationsStatsUpdate>,
    Path(guild_id): Path<String>,
) -> Result<Json<GuildDiscoveryResponse>, ApiError> {
    let guild_id = guild_id.trim();
    if guild_id.is_empty() || !guild_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::BadRequest);
    }
    let response =
        bot_api_get::<GuildDiscoveryResponse>(&format!("/guilds/{guild_id}/discovery")).await?;
    Ok(Json(response))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/configs", tag = "integrations", request_body = CreateDiscordConfigRequest, responses((status = 201, description = "Discord config created", body = DiscordConfigItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_config(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Json(payload): Json<CreateDiscordConfigRequest>,
) -> Result<(StatusCode, ApiJson<DiscordConfigItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest);
    }
    let item = integrations_repo::insert_discord_config(
        pool,
        &Uuid::new_v4().to_string(),
        payload.name.trim(),
        payload.guild_id.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(item, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/configs/{config_id}", tag = "integrations", params(("config_id" = String, Path, description = "Discord config ID")), request_body = UpdateDiscordConfigRequest, responses((status = 200, description = "Updated Discord config", body = DiscordConfigItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "Discord config not found")))]
pub async fn update_discord_config(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(config_id): Path<String>,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateDiscordConfigRequest>,
) -> Result<ApiJson<DiscordConfigItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = integrations_repo::update_discord_config_row(
        pool,
        &config_id,
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload.guild_id.is_some(),
        payload.guild_id.flatten(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(item, time))
}

#[utoipa::path(delete, path = "/api/v1/admin/integrations/discord/configs/{config_id}", tag = "integrations", params(("config_id" = String, Path, description = "Discord config ID")), responses((status = 200, description = "Deleted Discord config", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn delete_discord_config(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(config_id): Path<String>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    integrations_repo::delete_discord_config_row(pool, &config_id).await?;
    Ok(Json(ApiMessageBody {
        message: "discord config deleted".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/channels", tag = "integrations", request_body = CreateDiscordChannelRequest, responses((status = 201, description = "Discord channel created", body = DiscordChannelItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_channel(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Json(payload): Json<CreateDiscordChannelRequest>,
) -> Result<(StatusCode, ApiJson<DiscordChannelItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = integrations_repo::insert_discord_channel(
        pool,
        &Uuid::new_v4().to_string(),
        &payload.discord_config_id,
        payload.name.trim(),
        payload.channel_id.trim(),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(item, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/channels/{channel_id}", tag = "integrations", params(("channel_id" = String, Path, description = "Discord channel ID")), request_body = UpdateDiscordChannelRequest, responses((status = 200, description = "Updated Discord channel", body = DiscordChannelItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "Discord channel not found")))]
pub async fn update_discord_channel(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(channel_id): Path<String>,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateDiscordChannelRequest>,
) -> Result<ApiJson<DiscordChannelItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = integrations_repo::update_discord_channel_row(
        pool,
        &channel_id,
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload
            .channel_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(item, time))
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
    integrations_repo::delete_discord_channel_row(pool, &channel_id).await?;
    Ok(Json(ApiMessageBody {
        message: "discord channel deleted".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/roles", tag = "integrations", request_body = CreateDiscordRoleRequest, responses((status = 201, description = "Discord role created", body = DiscordRoleItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_role(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Json(payload): Json<CreateDiscordRoleRequest>,
) -> Result<(StatusCode, ApiJson<DiscordRoleItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = integrations_repo::insert_discord_role(
        pool,
        &Uuid::new_v4().to_string(),
        &payload.discord_config_id,
        payload.name.trim(),
        payload.role_id.trim(),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(item, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/roles/{role_id}", tag = "integrations", params(("role_id" = String, Path, description = "Discord role ID")), request_body = UpdateDiscordRoleRequest, responses((status = 200, description = "Updated Discord role", body = DiscordRoleItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "Discord role not found")))]
pub async fn update_discord_role(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(role_id): Path<String>,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateDiscordRoleRequest>,
) -> Result<ApiJson<DiscordRoleItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = integrations_repo::update_discord_role_row(
        pool,
        &role_id,
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload
            .role_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(item, time))
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
    integrations_repo::delete_discord_role_row(pool, &role_id).await?;
    Ok(Json(ApiMessageBody {
        message: "discord role deleted".to_string(),
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/categories", tag = "integrations", request_body = CreateDiscordCategoryRequest, responses((status = 201, description = "Discord category created", body = DiscordCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated")))]
pub async fn create_discord_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Json(payload): Json<CreateDiscordCategoryRequest>,
) -> Result<(StatusCode, ApiJson<DiscordCategoryItem>), ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = integrations_repo::insert_discord_category(
        pool,
        &Uuid::new_v4().to_string(),
        &payload.discord_config_id,
        payload.name.trim(),
        payload.category_id.trim(),
    )
    .await?;
    Ok((StatusCode::CREATED, ApiJson::new(item, time)))
}

#[utoipa::path(patch, path = "/api/v1/admin/integrations/discord/categories/{category_id}", tag = "integrations", params(("category_id" = String, Path, description = "Discord category ID")), request_body = UpdateDiscordCategoryRequest, responses((status = 200, description = "Updated Discord category", body = DiscordCategoryItem), (status = 400, description = "Invalid request"), (status = 401, description = "Not authenticated"), (status = 404, description = "Discord category not found")))]
pub async fn update_discord_category(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(category_id): Path<String>,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateDiscordCategoryRequest>,
) -> Result<ApiJson<DiscordCategoryItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let item = integrations_repo::update_discord_category_row(
        pool,
        &category_id,
        payload
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        payload
            .category_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(item, time))
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
    integrations_repo::delete_discord_category_row(pool, &category_id).await?;
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
                    // Announcements are gated by the `announcements` suppression
                    // category in resolve_recipients (opt-out), not by the retired
                    // event-notifications profile flag — send to all active users and
                    // let per-category suppression drop those who opted out.
                    receive_event_notifications: None,
                    active_only: Some(true),
                },
            )
            .await;
    }
    if payload.send_discord.unwrap_or(true) {
        integrations_repo::enqueue_outbound_job(
            pool,
            "discord.announcement",
            Some("announcement"),
            None,
            json!({
                "title": payload.title,
                "body_markdown": payload.body_markdown,
                "details_url": payload.details_url,
                "requested_by_cid": user.cid,
                "channel": payload.channel
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
    integrations_repo::enqueue_outbound_job(
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

#[utoipa::path(post, path = "/api/v1/events/{event_id}/discord-event", tag = "integrations", params(("event_id" = String, Path, description = "Event ID")), request_body = CreateDiscordScheduledEventRequest, responses((status = 200, description = "Discord scheduled-event creation queued", body = ApiMessageBody), (status = 401, description = "Not authenticated")))]
pub async fn queue_event_discord_scheduled_event(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(event_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<CreateDiscordScheduledEventRequest>,
) -> Result<Json<ApiMessageBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let location = payload
        .location
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("vatsim.net");
    integrations_repo::enqueue_outbound_job(
        pool,
        "discord.scheduled_event",
        Some("event"),
        Some(&event_id),
        json!({
            "event_id": event_id,
            "location": location,
            "requested_by_cid": user.cid
        }),
    )
    .await?;
    record_audit(
        pool,
        user,
        &headers,
        "QUEUE",
        "EVENT_DISCORD_SCHEDULED_EVENT",
        Some(event_id.clone()),
        None,
        Some(json!({ "event_id": event_id })),
    )
    .await?;
    Ok(Json(ApiMessageBody {
        message: "discord scheduled event queued".to_string(),
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/integrations/outbound-jobs", tag = "integrations", params(OutboundJobsQuery), responses((status = 200, description = "Outbound integration jobs", body = OutboundJobListResponse), (status = 401, description = "Not authenticated")))]
pub async fn list_outbound_jobs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Query(query): Query<OutboundJobsQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<OutboundJobListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 200);
    let total = integrations_repo::count_outbound_jobs(pool, query.status.as_deref()).await?;
    let rows = integrations_repo::list_outbound_jobs(
        pool,
        query.status.as_deref(),
        pagination.page_size,
        pagination.offset,
    )
    .await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);
    Ok(ApiJson::new(
        OutboundJobListResponse {
            items: rows,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/integrations/outbound-jobs/run", tag = "integrations", responses((status = 200, description = "Attempted outbound integration job deliveries", body = [OutboundJobItem]), (status = 401, description = "Not authenticated")))]
pub async fn run_outbound_jobs(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Vec<OutboundJobItem>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_integrations_manage(&state, user).await?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let mut rows = integrations_repo::list_pending_outbound_jobs(pool).await?;
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
        let updated = integrations_repo::update_outbound_job_result(
            pool,
            &row.id,
            &status,
            attempt_count,
            next_attempt_at,
            error,
        )
        .await?;
        *row = updated;
    }
    Ok(ApiJson::new(rows, time))
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

/// Shared, connection-pooled HTTP client for calls to the Discord bot, so each
/// request reuses keep-alive connections instead of building a fresh client.
pub(crate) static BOT_CLIENT: std::sync::LazyLock<Client> = std::sync::LazyLock::new(Client::new);

/// Perform an authenticated POST against the Discord bot's HTTP API and decode
/// the JSON response. Shares the `BOT_API_BASE_URL` / `BOT_API_SECRET_KEY`
/// configuration and the pooled `BOT_CLIENT`.
pub(crate) async fn bot_api_post<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let base_url = std::env::var("BOT_API_BASE_URL").map_err(|_| ApiError::ServiceUnavailable)?;
    let api_key = std::env::var("BOT_API_SECRET_KEY").map_err(|_| ApiError::ServiceUnavailable)?;

    let response = BOT_CLIENT
        .post(format!("{base_url}{path}"))
        .header("X-API-Key", api_key)
        .json(body)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(ApiError::ServiceUnavailable);
    }

    response
        .json::<T>()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)
}

/// Perform an authenticated GET against the Discord bot's HTTP API and decode
/// the JSON response. Shares the `BOT_API_BASE_URL` / `BOT_API_SECRET_KEY`
/// configuration used for outbound job delivery.
async fn bot_api_get<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let base_url = std::env::var("BOT_API_BASE_URL").map_err(|_| ApiError::ServiceUnavailable)?;
    let api_key = std::env::var("BOT_API_SECRET_KEY").map_err(|_| ApiError::ServiceUnavailable)?;

    let response = BOT_CLIENT
        .get(format!("{base_url}{path}"))
        .header("X-API-Key", api_key)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(ApiError::ServiceUnavailable);
    }

    response
        .json::<T>()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)
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
        "discord.scheduled_event" => ("/scheduled_event", job.payload.clone()),
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

/// Reads a trimmed Discord OAuth setting, treating an empty value as unset: `.env.example`
/// ships these keys blank, and an empty `client_id` would otherwise produce an
/// authorize URL Discord rejects instead of the `auth_url: null` clients handle.
fn discord_env(name: &'static str) -> Option<String> {
    let value = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if value.is_none() {
        tracing::warn!(var = name, "discord linking not configured");
    }
    value
}

async fn exchange_discord_code(
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<DiscordUserIdentity, ApiError> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| ApiError::ServiceUnavailable)?;
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
        .map_err(|err| {
            tracing::warn!(error = %err, "discord token exchange request failed");
            ApiError::ServiceUnavailable
        })?;
    if !token.status().is_success() {
        let status = token.status();
        let body = token
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_string());
        tracing::warn!(
            %status,
            redirect_uri,
            %body,
            "discord token exchange rejected"
        );
        return Err(ApiError::BadRequest);
    }
    let token_body = token.json::<DiscordTokenResponse>().await.map_err(|err| {
        tracing::warn!(error = %err, "discord token response unreadable");
        ApiError::ServiceUnavailable
    })?;

    let identity = client
        .get("https://discord.com/api/users/@me")
        .bearer_auth(&token_body.access_token)
        .send()
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "discord identity request failed");
            ApiError::ServiceUnavailable
        })?;
    if !identity.status().is_success() {
        tracing::warn!(status = %identity.status(), "discord identity request rejected");
        return Err(ApiError::BadRequest);
    }
    identity.json::<DiscordUserIdentity>().await.map_err(|err| {
        tracing::warn!(error = %err, "discord identity response unreadable");
        ApiError::ServiceUnavailable
    })
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
            message: None,
            before_state,
            after_state,
            ip_address: audit_repo::client_ip(headers),
        },
    )
    .await
}
