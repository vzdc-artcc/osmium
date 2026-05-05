use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Redirect,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono_tz::Tz;
use reqwest::Url;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{
            PermissionAction, PermissionPath, fetch_service_account_access, fetch_user_access,
            is_server_admin, permission_tree_from_paths,
        },
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
        vatsim::{VatsimOAuthConfig, exchange_code_for_token, fetch_profile},
    },
    errors::ApiError,
    models::{
        CreateTeamSpeakUidRequest, MeBody, PatchMeRequest, ServiceAccountSessionBody,
        TeamSpeakUidBody,
    },
    repos::{access as access_repo, users as user_repo},
    state::AppState,
};

const OAUTH_STATE_COOKIE: &str = "osmium_oauth_state";
const SESSION_COOKIE: &str = "osmium_session";
const OAUTH_STATE_TTL_SECS: i64 = 10 * 60;
const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 30;

#[derive(Deserialize, ToSchema)]
pub struct LoginQuery {
    prompt: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current authenticated user session", body = MeBody),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn me(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<MeBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;
    Ok(Json(build_me_body(&state, user).await?))
}

#[utoipa::path(
    patch,
    path = "/api/v1/me",
    tag = "auth",
    request_body(
        content = PatchMeRequest,
        description = "Self-service profile updates only. This route cannot change roles, permissions, or access overrides. Use POST /api/v1/admin/users/{cid}/access for access changes."
    ),
    responses(
        (status = 200, description = "Updated current user profile", body = MeBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn patch_me(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<PatchMeRequest>,
) -> Result<Json<MeBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Update),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let current_profile = user_repo::fetch_me_profile(pool, &user.id).await?;

    let preferred_name = payload
        .preferred_name
        .map(normalize_optional_text)
        .unwrap_or(current_profile.preferred_name);
    let bio = payload
        .bio
        .map(normalize_optional_text)
        .unwrap_or(current_profile.bio);
    let timezone = match payload.timezone {
        Some(value) => validate_timezone(&value)?,
        None => current_profile.timezone,
    };
    let receive_event_notifications = payload
        .receive_event_notifications
        .unwrap_or(current_profile.receive_event_notifications);

    user_repo::update_me_profile(
        pool,
        &user.id,
        &user_repo::SelfProfileUpdate {
            preferred_name,
            bio,
            timezone,
            receive_event_notifications,
        },
    )
    .await?;

    Ok(Json(build_me_body(&state, user).await?))
}

#[utoipa::path(
    get,
    path = "/api/v1/me/teamspeak-uids",
    tag = "auth",
    responses(
        (status = 200, description = "Current user's TeamSpeak UIDs", body = [TeamSpeakUidBody]),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn list_my_teamspeak_uids(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<Vec<TeamSpeakUidBody>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "teamspeak_uids"], PermissionAction::Read),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(Json(user_repo::list_teamspeak_uids(pool, &user.id).await?))
}

#[utoipa::path(
    post,
    path = "/api/v1/me/teamspeak-uids",
    tag = "auth",
    request_body(
        content = CreateTeamSpeakUidRequest,
        description = "Self-service TeamSpeak UID linkage only. This route does not manage permissions or any other user access state."
    ),
    responses(
        (status = 200, description = "Added TeamSpeak UID", body = TeamSpeakUidBody),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn create_my_teamspeak_uid(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Json(payload): Json<CreateTeamSpeakUidRequest>,
) -> Result<Json<TeamSpeakUidBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "teamspeak_uids"], PermissionAction::Create),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let uid = payload.uid.trim();
    if uid.is_empty() {
        return Err(ApiError::BadRequest);
    }

    Ok(Json(
        user_repo::create_teamspeak_uid(pool, &user.id, uid).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/me/teamspeak-uids/{identity_id}",
    tag = "auth",
    params(
        ("identity_id" = String, Path, description = "TeamSpeak identity id")
    ),
    responses(
        (status = 204, description = "Deleted TeamSpeak UID"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn delete_my_teamspeak_uid(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(identity_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "teamspeak_uids"], PermissionAction::Delete),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    user_repo::delete_teamspeak_uid(pool, &user.id, &identity_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/service-account/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current authenticated service account", body = crate::models::ServiceAccountSessionBody),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn service_account_me(
    State(state): State<AppState>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<ServiceAccountSessionBody>, ApiError> {
    let service_account = current_service_account
        .as_ref()
        .ok_or(ApiError::Unauthorized)?;
    let (roles, permissions) =
        fetch_service_account_access(state.db.as_ref(), &service_account.id).await?;

    Ok(Json(ServiceAccountSessionBody {
        id: service_account.id.clone(),
        key: service_account.key.clone(),
        name: service_account.name.clone(),
        roles,
        permissions: permission_tree_from_paths(&permissions),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/vatsim/login",
    tag = "auth",
    params(
        ("prompt" = Option<String>, Query, description = "Optional OAuth prompt override")
    ),
    responses(
        (status = 307, description = "Redirects to VATSIM OAuth")
    )
)]
pub async fn vatsim_login(
    jar: CookieJar,
    headers: HeaderMap,
    Query(query): Query<LoginQuery>,
) -> Result<(CookieJar, Redirect), ApiError> {
    let config = VatsimOAuthConfig::from_env()?;
    validate_oauth_login_origin(&headers, &config)?;
    let oauth_state = Uuid::new_v4().to_string();

    let mut authorize_url =
        Url::parse(&config.authorization_url(&oauth_state)?).map_err(|_| ApiError::Internal)?;

    if let Some(prompt) = parse_prompt(query.prompt.as_deref())? {
        authorize_url
            .query_pairs_mut()
            .append_pair("prompt", prompt);
    }

    let state_cookie = Cookie::build((OAUTH_STATE_COOKIE, oauth_state.clone()))
        .http_only(true)
        .secure(cookie_secure())
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(OAUTH_STATE_TTL_SECS))
        .build();

    Ok((
        jar.add(state_cookie),
        Redirect::temporary(&authorize_url.to_string()),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/vatsim/callback",
    tag = "auth",
    params(
        ("code" = Option<String>, Query, description = "OAuth authorization code"),
        ("state" = Option<String>, Query, description = "OAuth state token")
    ),
    responses(
        (status = 302, description = "Completes login and redirects to /api/v1/me"),
        (status = 400, description = "Invalid callback state or code")
    )
)]
pub async fn vatsim_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<CallbackQuery>,
) -> Result<(CookieJar, Redirect), ApiError> {
    let code = query
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::BadRequest)?;

    let callback_state = query
        .state
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::BadRequest)?;

    let Some(cookie_state) = jar.get(OAUTH_STATE_COOKIE).map(|cookie| cookie.value()) else {
        tracing::warn!("oauth callback missing state cookie");
        return Err(ApiError::OAuthStateCookieMissing);
    };

    if cookie_state != callback_state {
        tracing::warn!("oauth callback state mismatch");
        return Err(ApiError::OAuthStateMismatch);
    }

    let config = VatsimOAuthConfig::from_env()?;
    let access_token = exchange_code_for_token(&config, code).await?;
    let profile = fetch_profile(&config, &access_token).await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let user_id = bootstrap_login_user(
        pool,
        profile.cid,
        &profile.email,
        &profile.display_name,
        &profile.display_name,
        profile.rating.as_deref(),
    )
    .await?;

    tracing::info!(
        cid = profile.cid,
        user_id = user_id.as_str(),
        source = "vatsim_oauth",
        rating = profile.rating.as_deref(),
        "oauth user sync completed"
    );

    ensure_user_login_access(pool, &user_id, profile.cid)
        .await
        .map_err(|error| {
            tracing::error!(
                ?error,
                user_id = user_id.as_str(),
                cid = profile.cid,
                "failed to ensure user access during oauth callback"
            );
            error
        })?;

    let session_token = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into identity.sessions (session_token, user_id, expires_at)
        values ($1, $2, now() + interval '30 days')
        "#,
    )
    .bind(&session_token)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(
            ?error,
            user_id = user_id.as_str(),
            "failed to create session during oauth callback"
        );
        ApiError::Internal
    })?;

    let clear_state_cookie = Cookie::build((OAUTH_STATE_COOKIE, ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    let session_cookie = Cookie::build((SESSION_COOKIE, session_token))
        .http_only(true)
        .secure(cookie_secure())
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(SESSION_TTL_SECS))
        .build();

    Ok((
        jar.remove(clear_state_cookie).add(session_cookie),
        Redirect::to("/api/v1/me"),
    ))
}

pub async fn login_as_cid(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), ApiError> {
    if !api_dev_mode_enabled() {
        return Err(ApiError::Unauthorized);
    }

    if cid <= 0 {
        return Err(ApiError::BadRequest);
    }

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let generated_email = format!("dev-cid-{}@example.invalid", cid);
    let generated_name = format!("Dev CID {}", cid);

    let user_id = bootstrap_login_user(
        pool,
        cid,
        &generated_email,
        &generated_name,
        &generated_name,
        None,
    )
    .await?;

    ensure_user_login_access(pool, &user_id, cid)
        .await
        .map_err(|error| {
            tracing::error!(
                ?error,
                user_id = user_id.as_str(),
                cid,
                "failed to ensure user access during dev cid login"
            );
            error
        })?;

    let session_token = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into identity.sessions (session_token, user_id, expires_at)
        values ($1, $2, now() + interval '30 days')
        "#,
    )
    .bind(&session_token)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(
            ?error,
            user_id = user_id.as_str(),
            "failed to create session during dev cid login"
        );
        ApiError::Internal
    })?;

    let session_cookie = Cookie::build((SESSION_COOKIE, session_token))
        .http_only(true)
        .secure(cookie_secure())
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(SESSION_TTL_SECS))
        .build();

    Ok((jar.add(session_cookie), Redirect::to("/api/v1/me")))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    responses(
        (status = 204, description = "Session revoked"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn logout(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(session_token): Extension<Option<String>>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), ApiError> {
    ensure_permission(
        &state,
        current_user.as_ref(),
        None,
        PermissionPath::from_segments(["auth", "sessions"], PermissionAction::Delete),
    )
    .await?;

    if let (Some(pool), Some(token)) = (state.db.as_ref(), session_token.as_deref()) {
        sqlx::query("delete from identity.sessions where session_token = $1")
            .bind(token)
            .execute(pool)
            .await
            .map_err(|_| ApiError::Internal)?;
    }

    let session_cookie = Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    Ok((jar.remove(session_cookie), StatusCode::NO_CONTENT))
}

fn parse_prompt(raw_prompt: Option<&str>) -> Result<Option<&str>, ApiError> {
    let Some(prompt) = raw_prompt.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    match prompt {
        "none" | "login" | "consent" => Ok(Some(prompt)),
        _ => Err(ApiError::BadRequest),
    }
}

async fn build_me_body(state: &AppState, user: &CurrentUser) -> Result<MeBody, ApiError> {
    ensure_permission(
        state,
        Some(user),
        None,
        PermissionPath::from_segments(["auth", "profile"], PermissionAction::Read),
    )
    .await?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    let profile = user_repo::fetch_me_profile(pool, &user.id).await?;
    let teamspeak_uids = user_repo::list_teamspeak_uids(pool, &user.id).await?;

    Ok(MeBody {
        id: user.id.clone(),
        cid: user.cid,
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        rating: user.rating.clone(),
        server_admin: is_server_admin(&roles),
        permissions: permission_tree_from_paths(&permissions),
        profile,
        teamspeak_uids,
    })
}

async fn bootstrap_login_user(
    pool: &sqlx::PgPool,
    cid: i64,
    email: &str,
    full_name: &str,
    display_name: &str,
    rating: Option<&str>,
) -> Result<String, ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let user = user_repo::upsert_login_user(
        &mut tx,
        &Uuid::new_v4().to_string(),
        cid,
        email,
        full_name,
        display_name,
    )
    .await?;

    user_repo::ensure_user_profile(&mut tx, &user.id).await?;
    user_repo::upsert_login_membership(&mut tx, &user.id, rating).await?;
    user_repo::ensure_operating_initials(
        &mut tx,
        &user.id,
        user.first_name.as_deref(),
        user.last_name.as_deref(),
        display_name,
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(user.id)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|inner| inner.trim().to_string())
        .filter(|inner| !inner.is_empty())
}

fn validate_timezone(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(ApiError::BadRequest);
    }

    normalized.parse::<Tz>().map_err(|_| ApiError::BadRequest)?;
    Ok(normalized.to_string())
}

async fn ensure_user_login_access(
    pool: &sqlx::PgPool,
    user_id: &str,
    cid: i64,
) -> Result<(), ApiError> {
    let configured_server_admin_cid = configured_server_admin_cid();

    match configured_server_admin_cid {
        Some(server_admin_cid) if server_admin_cid == cid => {
            tracing::info!(
                user_id,
                cid,
                configured_server_admin_cid = server_admin_cid,
                "assigning server admin role during login sync"
            );

            let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
            access_repo::assign_server_admin(&mut tx, user_id).await?;
            tx.commit().await.map_err(|_| ApiError::Internal)?;

            let roles = access_repo::fetch_user_role_names(pool, user_id).await?;
            tracing::info!(
                user_id,
                cid,
                roles = ?roles,
                "server admin login access synced"
            );

            Ok(())
        }
        _ => {
            let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
            access_repo::replace_user_permissions(
                &mut tx,
                user_id,
                &[
                    "auth.profile.read".to_string(),
                    "auth.profile.update".to_string(),
                    "auth.teamspeak_uids.read".to_string(),
                    "auth.teamspeak_uids.create".to_string(),
                    "auth.teamspeak_uids.delete".to_string(),
                    "auth.sessions.delete".to_string(),
                    "users.vatusa_refresh.self.request".to_string(),
                    "users.visit_artcc.request".to_string(),
                    "users.visitor_applications.self.read".to_string(),
                    "users.visitor_applications.self.request".to_string(),
                    "feedback.items.self.read".to_string(),
                    "feedback.items.create".to_string(),
                    "events.positions.self.request".to_string(),
                ],
            )
            .await?;
            tx.commit().await.map_err(|_| ApiError::Internal)?;

            tracing::info!(
                user_id,
                cid,
                configured_server_admin_cid,
                "default user login access synced"
            );

            Ok(())
        }
    }
}

fn configured_server_admin_cid() -> Option<i64> {
    let raw = std::env::var("OSMIUM_SERVER_ADMIN_CID").ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    raw.parse::<i64>().ok().filter(|cid| *cid > 0)
}

fn cookie_secure() -> bool {
    std::env::var("COOKIE_SECURE")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn validate_oauth_login_origin(
    headers: &HeaderMap,
    config: &VatsimOAuthConfig,
) -> Result<(), ApiError> {
    let expected_origin = url_origin(&config.redirect_uri).ok_or(ApiError::Internal)?;
    let Some(request_origin) = request_origin(headers) else {
        tracing::warn!(
            expected_origin,
            "oauth login request missing host/origin headers"
        );
        return Err(ApiError::OAuthLoginOriginMismatch);
    };

    if request_origin != expected_origin {
        tracing::warn!(
            expected_origin,
            request_origin,
            "oauth login request origin does not match configured redirect origin"
        );
        return Err(ApiError::OAuthLoginOriginMismatch);
    }

    Ok(())
}

fn request_origin(headers: &HeaderMap) -> Option<String> {
    if let Some(origin) = header_value(headers, "origin") {
        return Some(origin);
    }

    let host =
        header_value(headers, "x-forwarded-host").or_else(|| header_value(headers, "host"))?;
    let proto = header_value(headers, "x-forwarded-proto").unwrap_or_else(|| "http".to_string());
    Some(format!("{proto}://{host}"))
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn url_origin(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let host = url.host_str()?;
    let scheme = url.scheme();
    let port = url.port_or_known_default()?;

    let is_default_port = (scheme == "http" && port == 80) || (scheme == "https" && port == 443);
    if is_default_port {
        Some(format!("{scheme}://{host}"))
    } else {
        Some(format!("{scheme}://{host}:{port}"))
    }
}

fn api_dev_mode_enabled() -> bool {
    env_flag_enabled("API_DEV_MODE") || env_flag_enabled("VATSIM_DEV_MODE")
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::configured_server_admin_cid;
    use crate::auth::acl::SERVER_ADMIN_ROLE;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }

            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::remove_var(key);
            }

            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.as_deref() {
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    fn env_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn parses_configured_server_admin_cid() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("OSMIUM_SERVER_ADMIN_CID", "1234567");

        assert_eq!(configured_server_admin_cid(), Some(1234567));
    }

    #[test]
    fn ignores_invalid_configured_server_admin_cid() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("OSMIUM_SERVER_ADMIN_CID", "abc");

        assert_eq!(configured_server_admin_cid(), None);
    }

    #[test]
    fn ignores_missing_configured_server_admin_cid() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::unset("OSMIUM_SERVER_ADMIN_CID");

        assert_eq!(configured_server_admin_cid(), None);
    }

    #[test]
    fn server_admin_role_constant_is_stable() {
        assert_eq!(SERVER_ADMIN_ROLE, "SERVER_ADMIN");
    }
}
