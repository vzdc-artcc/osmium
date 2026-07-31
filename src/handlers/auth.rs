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
            fetch_service_account_access, fetch_user_access, is_server_admin,
            permission_tree_from_paths,
        },
        context::{CurrentServiceAccount, CurrentUser, SessionToken},
        permissions::{
            AuthImpersonateCreate, AuthProfileRead, AuthProfileUpdate, AuthSessionsDelete,
            AuthTeamspeakUidsCreate, AuthTeamspeakUidsDelete, AuthTeamspeakUidsRead,
        },
        require_permission::RequirePermission,
        vatsim::{VatsimOAuthConfig, exchange_code_for_token, fetch_profile},
    },
    errors::ApiError,
    models::{
        CreateTeamSpeakUidRequest, ImpersonationBanner, MeBody, PatchMeRequest,
        ServiceAccountSessionBody, TeamSpeakLookupRequest, TeamSpeakLookupResponse,
        TeamSpeakUidBody,
    },
    repos::{access as access_repo, users as user_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

const OAUTH_STATE_COOKIE: &str = "osmium_oauth_state";
const OAUTH_RETURN_TO_COOKIE: &str = "osmium_oauth_return_to";
const SESSION_COOKIE: &str = "osmium_session";
const OAUTH_STATE_TTL_SECS: i64 = 10 * 60;
const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 30;
const DEFAULT_LOGIN_REDIRECT: &str = "/api/v1/me";

#[derive(Deserialize, ToSchema)]
pub struct LoginQuery {
    prompt: Option<String>,
    return_to: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct StartImpersonationRequest {
    /// Optional free-text reason, recorded in the server-level audit entry.
    #[serde(default)]
    pub reason: Option<String>,
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
    _permission: RequirePermission<AuthProfileRead>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
) -> Result<ApiJson<MeBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    Ok(ApiJson::new(build_me_body(&state, user).await?, time))
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
    _permission: RequirePermission<AuthProfileUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Json(payload): Json<PatchMeRequest>,
) -> Result<ApiJson<MeBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

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

    user_repo::update_me_profile(
        pool,
        &user.id,
        &user_repo::SelfProfileUpdate {
            preferred_name,
            bio,
            timezone,
        },
    )
    .await?;

    if let Some(initials) = payload.operating_initials {
        let normalized = initials.trim().to_ascii_uppercase();
        if normalized.len() != 2 || !normalized.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(ApiError::BadRequest);
        }
        let assigned = user_repo::reassign_operating_initials(pool, &user.id, &normalized).await?;
        if !assigned {
            return Err(ApiError::Conflict);
        }
    }

    Ok(ApiJson::new(build_me_body(&state, user).await?, time))
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
    _permission: RequirePermission<AuthTeamspeakUidsRead>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
) -> Result<ApiJson<Vec<TeamSpeakUidBody>>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(ApiJson::new(
        user_repo::list_teamspeak_uids(pool, &user.id).await?,
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/integrations/teamspeak/lookup",
    tag = "auth",
    request_body(content = TeamSpeakLookupRequest, description = "TeamSpeak client UID to resolve to a controller"),
    responses(
        (status = 200, description = "Controller identity + live position for the UID", body = TeamSpeakLookupResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "No controller linked to that UID")
    )
)]
pub async fn lookup_teamspeak_controller(
    State(state): State<AppState>,
    _permission: RequirePermission<AuthTeamspeakUidsRead>,
    time: ResponseTimeContext,
    Json(payload): Json<TeamSpeakLookupRequest>,
) -> Result<ApiJson<TeamSpeakLookupResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let result = user_repo::lookup_teamspeak_controller(pool, &payload.uid)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(ApiJson::new(result, time))
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
    _permission: RequirePermission<AuthTeamspeakUidsCreate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Json(payload): Json<CreateTeamSpeakUidRequest>,
) -> Result<ApiJson<TeamSpeakUidBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let uid = payload.uid.trim();
    if uid.is_empty() {
        return Err(ApiError::BadRequest);
    }

    Ok(ApiJson::new(
        user_repo::create_teamspeak_uid(pool, &user.id, uid).await?,
        time,
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
    _permission: RequirePermission<AuthTeamspeakUidsDelete>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Path(identity_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;

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
        ("prompt" = Option<String>, Query, description = "Optional OAuth prompt override"),
        ("return_to" = Option<String>, Query, description = "Absolute URL to redirect to after login completes. Origin must be in CORS_ALLOWED_ORIGINS; defaults to /api/v1/me if omitted or invalid.")
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

    let mut jar = jar.add(state_cookie);

    if let Some(raw_return_to) = query
        .return_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let return_to = validate_return_to(raw_return_to)?;
        let return_to_cookie = Cookie::build((OAUTH_RETURN_TO_COOKIE, return_to))
            .http_only(true)
            .secure(cookie_secure())
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(time::Duration::seconds(OAUTH_STATE_TTL_SECS))
            .build();
        jar = jar.add(return_to_cookie);
    }

    Ok((jar, Redirect::temporary(&authorize_url.to_string())))
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
        (status = 302, description = "Completes login and redirects to the validated return_to target from the login request, or /api/v1/me if none was provided"),
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

    let (user_id, was_new_user) = bootstrap_login_user(
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

    ensure_user_login_access(pool, &user_id, profile.cid, was_new_user)
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
    crate::repos::auth::insert_session(pool, &session_token, &user_id)
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

    let redirect_target = jar
        .get(OAUTH_RETURN_TO_COOKIE)
        .map(|cookie| cookie.value().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| match validate_return_to(&value) {
            Ok(validated) => Some(validated),
            Err(_) => {
                tracing::warn!(
                    return_to = value.as_str(),
                    "ignoring invalid return_to cookie at oauth callback"
                );
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_LOGIN_REDIRECT.to_string());

    let clear_return_to_cookie = Cookie::build((OAUTH_RETURN_TO_COOKIE, ""))
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
        jar.remove(clear_state_cookie)
            .remove(clear_return_to_cookie)
            .add(session_cookie),
        Redirect::to(&redirect_target),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/impersonate/{cid}",
    tag = "auth",
    params(("cid" = i64, Path, description = "VATSIM CID of the user to impersonate")),
    request_body = StartImpersonationRequest,
    responses(
        (status = 200, description = "Now impersonating the target; returns the target's /me view", body = MeBody),
        (status = 400, description = "Invalid target (self, or already impersonating)"),
        (status = 401, description = "Not authorized"),
        (status = 403, description = "Target is a server admin (refused)"),
        (status = 404, description = "Target user not found")
    )
)]
pub async fn start_impersonation(
    State(state): State<AppState>,
    _permission: RequirePermission<AuthImpersonateCreate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(SessionToken(session_token)): Extension<SessionToken>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Path(cid): Path<i64>,
    Json(payload): Json<StartImpersonationRequest>,
) -> Result<ApiJson<MeBody>, ApiError> {
    // Human session only — a service account can neither reach here (no CurrentUser)
    // nor hold the permission.
    let admin = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let token = session_token.as_deref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    // Refuse nested impersonation (checklist #3). Also implicitly enforced by the
    // permission check above (the impersonated subject won't hold the permission),
    // but made explicit here.
    if admin.is_impersonated() {
        return Err(ApiError::BadRequest);
    }

    if cid <= 0 || cid == admin.cid {
        return Err(ApiError::BadRequest);
    }

    let target_user_id = access_repo::find_user_id_by_cid(pool, cid)
        .await?
        .ok_or(ApiError::NotFound)?;
    if target_user_id == admin.id {
        return Err(ApiError::BadRequest);
    }

    // Refuse impersonating a server admin (or any target that would not de-escalate)
    // — privilege elevation guard (checklist #3).
    let (target_roles, _) = fetch_user_access(state.db.as_ref(), &target_user_id).await?;
    if is_server_admin(&target_roles) {
        return Err(ApiError::Forbidden);
    }

    let reason = payload
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let started = access_repo::start_impersonation(
        pool,
        token,
        &target_user_id,
        &admin.id,
        reason,
        crate::config::impersonation_ttl_secs(),
    )
    .await?;
    if !started {
        // The conditional update's guards rejected it (e.g. a concurrent change).
        return Err(ApiError::Conflict);
    }

    // Provision the target with the baseline self-service permissions every
    // controller is entitled to. These are otherwise only materialized on a user's
    // first login, so a target who has never logged in (dev-seeded, migrated) would
    // resolve with *fewer* permissions than they should — meaning impersonation
    // couldn't perform basic self-service (e.g. requesting an event position).
    // Additive and revoke-preserving: impersonation reflects exactly the target's
    // real permissions (nothing less), never the admin's (nothing more).
    access_repo::grant_missing_permissions(
        pool,
        &target_user_id,
        BASELINE_SELF_SERVICE_PERMISSIONS,
    )
    .await?;

    // Server-level audit, attributed to the real admin, target in metadata. Recorded
    // with the AUTH_IMPERSONATION resource type so facility admins never see it.
    record_impersonation_audit(pool, &headers, admin, "START", cid, reason).await?;

    // Re-resolve the (now impersonating) session and return the target's /me view.
    let target = access_repo::find_current_user_by_session_token(pool, token)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok(ApiJson::new(build_me_body(&state, &target).await?, time))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/impersonate/stop",
    tag = "auth",
    responses(
        (status = 200, description = "Impersonation ended; returns the restored admin's /me view", body = MeBody),
        (status = 400, description = "Not currently impersonating"),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn stop_impersonation(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(SessionToken(session_token)): Extension<SessionToken>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<MeBody>, ApiError> {
    // No permission gate: the impersonated session's effective user is the target,
    // who may hold nothing. The only requirement is that this session is impersonating.
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let token = session_token.as_deref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    if !user.is_impersonated() {
        return Err(ApiError::BadRequest);
    }
    let impersonated_cid = user.cid;
    let admin_user_id = user
        .impersonator_user_id
        .clone()
        .ok_or(ApiError::BadRequest)?;

    let restored = access_repo::stop_impersonation(pool, token, SESSION_TTL_SECS).await?;
    if restored.is_none() {
        return Err(ApiError::BadRequest);
    }

    // Re-resolve the restored admin session for the audit actor + response.
    let admin = access_repo::find_current_user_by_session_token(pool, token)
        .await?
        .ok_or(ApiError::Internal)?;
    let _ = admin_user_id;
    record_impersonation_audit(pool, &headers, &admin, "STOP", impersonated_cid, None).await?;

    Ok(ApiJson::new(build_me_body(&state, &admin).await?, time))
}

/// Records a server-level `AUTH_IMPERSONATION` audit row attributed to the real
/// admin, with the impersonated CID (and optional reason) in the after-state.
async fn record_impersonation_audit(
    pool: &sqlx::PgPool,
    headers: &HeaderMap,
    admin: &CurrentUser,
    action: &str,
    target_cid: i64,
    reason: Option<&str>,
) -> Result<(), ApiError> {
    let actor = crate::repos::audit::resolve_audit_actor(pool, Some(admin), None).await?;
    crate::repos::audit::record_audit(
        pool,
        crate::repos::audit::AuditEntryInput {
            actor_id: actor.actor_id,
            action: action.to_string(),
            resource_type: crate::repos::audit::AUTH_IMPERSONATION_RESOURCE.to_string(),
            resource_id: Some(target_cid.to_string()),
            scope_type: "global".to_string(),
            scope_key: Some(target_cid.to_string()),
            message: None,
            before_state: None,
            after_state: Some(serde_json::json!({
                "impersonated_cid": target_cid,
                "reason": reason,
            })),
            ip_address: crate::repos::audit::client_ip(headers),
        },
    )
    .await
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
    _permission: RequirePermission<AuthSessionsDelete>,
    Extension(SessionToken(session_token)): Extension<SessionToken>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), ApiError> {
    if let (Some(pool), Some(token)) = (state.db.as_ref(), session_token.as_deref()) {
        crate::repos::auth::delete_session(pool, token).await?;
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
    // Callers gate `auth.profile.read` via `RequirePermission<AuthProfileRead>`
    // before invoking this helper (currently only `me`).
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
    let profile = user_repo::fetch_me_profile(pool, &user.id).await?;
    let flags = user_repo::fetch_user_flags(pool, &user.id).await?;
    let controller_status = user_repo::fetch_controller_status(pool, &user.id).await?;
    let teamspeak_uids = user_repo::list_teamspeak_uids(pool, &user.id).await?;

    // Minimal real-actor identity for the impersonation banner + stop control.
    // `user` here is the *target* (impersonation resolves `id`/`cid` to them); the
    // impersonator fields carry the real admin.
    let impersonation = match (
        user.impersonator_cid,
        user.impersonator_display_name.as_ref(),
    ) {
        (Some(impersonator_cid), Some(display_name)) => Some(ImpersonationBanner {
            impersonator_cid,
            impersonator_display_name: display_name.clone(),
        }),
        _ => None,
    };

    Ok(MeBody {
        id: user.id.clone(),
        cid: user.cid,
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        rating: user.rating.clone(),
        controller_status,
        server_admin: is_server_admin(&roles),
        role_names: roles,
        permissions: permission_tree_from_paths(&permissions),
        profile,
        flags,
        teamspeak_uids,
        impersonation,
    })
}

/// Upserts the identity/profile/membership rows for a logging-in user, returning
/// `(user_id, was_new_user)`. Public so integration tests can exercise the login
/// bootstrap path directly now that the dev login-as route (which used to be the
/// test trigger) is retired; the real trigger is `vatsim_callback`.
pub async fn bootstrap_login_user(
    pool: &sqlx::PgPool,
    cid: i64,
    email: &str,
    full_name: &str,
    display_name: &str,
    rating: Option<&str>,
) -> Result<(String, bool), ApiError> {
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

    // Ensure this user has an audit actor so their actions (and IP history) attribute
    // to them instead of resolving to a null "system" actor.
    access_repo::ensure_user_actor(&mut tx, &user.id, display_name).await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok((user.id, user.was_new_user))
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

/// The self-service permissions every non-`SERVER_ADMIN` user is entitled to.
/// Seeded into `access.user_permissions` on a user's first login (below), and
/// provisioned onto an impersonation target so acting-as-them reflects their real
/// permissions even if they never logged in. Single source of truth so the login
/// baseline and the impersonation baseline can never drift apart.
pub(crate) const BASELINE_SELF_SERVICE_PERMISSIONS: &[&str] = &[
    "auth.profile.read",
    "auth.profile.update",
    "auth.teamspeak_uids.read",
    "auth.teamspeak_uids.create",
    "auth.teamspeak_uids.delete",
    "auth.sessions.delete",
    "users.vatusa_refresh.self.request",
    "users.visit_artcc.request",
    "users.visitor_applications.self.read",
    "users.visitor_applications.self.request",
    "users.directory.read",
    "feedback.items_self.read",
    "feedback.items.create",
    "events.positions.self.request",
];

/// Seeds baseline permissions once (only for a newly-created user) and keeps the
/// `OSMIUM_SERVER_ADMIN_CID` role sync idempotent on every login. Public for the
/// same integration-test reason as [`bootstrap_login_user`].
pub async fn ensure_user_login_access(
    pool: &sqlx::PgPool,
    user_id: &str,
    cid: i64,
    was_new_user: bool,
) -> Result<(), ApiError> {
    let configured_server_admin_cids = configured_server_admin_cids();

    if configured_server_admin_cids.contains(&cid) {
        tracing::info!(
            user_id,
            cid,
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
    } else {
        // Reconcile a demotion and (re)seed baseline access in a single
        // transaction so the two commit or roll back together. A user no longer
        // in OSMIUM_SERVER_ADMIN_CID must not keep the SERVER_ADMIN role granted
        // on a previous login; but since a former admin holds no other roles or
        // permissions, revoking outside the seed's transaction risks a crash
        // between them leaving the account with neither — a lockout no later
        // login would repair (the revoke would then be a no-op, so the demotion
        // is never re-detected). Sharing one transaction makes the failure mode
        // "keep SERVER_ADMIN, retry next login" instead.
        let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

        // No-op (zero rows) for users who never held the role.
        let demoted = access_repo::revoke_server_admin(&mut tx, user_id).await?;

        // Seed baseline self-service permissions when the identity.users row is
        // first created (was_new_user), or when we just demoted a former server
        // admin — whose only access was the now-removed role — so the account is
        // left as an ordinary user rather than locked out with no permissions.
        // Any other returning user leaves access.user_permissions untouched, so
        // admin-granted permissions (via the staff permissions editor) survive
        // across logins instead of being wiped back to the baseline each time.
        if was_new_user || demoted {
            access_repo::replace_user_permissions(
                &mut tx,
                user_id,
                &BASELINE_SELF_SERVICE_PERMISSIONS
                    .iter()
                    .map(|permission| permission.to_string())
                    .collect::<Vec<_>>(),
            )
            .await?;
        }

        tx.commit().await.map_err(|_| ApiError::Internal)?;

        if demoted {
            tracing::info!(
                user_id,
                cid,
                "revoked server admin role on login (cid no longer configured); reset to baseline access"
            );
        } else if was_new_user {
            tracing::info!(user_id, cid, "baseline login access seeded for new user");
        }

        Ok(())
    }
}

/// Parses `OSMIUM_SERVER_ADMIN_CID` into the set of CIDs that should hold the
/// SERVER_ADMIN role. Accepts a single CID or a comma-separated list, so more
/// than one person can be a server admin (e.g. `123,456,789`). Server admin is
/// env-configured only — it is never grantable through the permissions UI.
fn configured_server_admin_cids() -> Vec<i64> {
    let Ok(raw) = std::env::var("OSMIUM_SERVER_ADMIN_CID") else {
        return Vec::new();
    };
    raw.split(',')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .filter(|cid| *cid > 0)
        .collect()
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

/// Validates a `return_to` redirect target: must be an absolute http(s) URL
/// whose origin is in `CORS_ALLOWED_ORIGINS`. Rejecting anything else prevents
/// this login flow from being used as an open redirect.
fn validate_return_to(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let parsed = Url::parse(trimmed).map_err(|_| ApiError::BadRequest)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ApiError::BadRequest);
    }

    let origin = url_origin(trimmed).ok_or(ApiError::BadRequest)?;
    let allowed_origins = crate::config::configured_allowed_origins();
    if !allowed_origins.iter().any(|allowed| allowed == &origin) {
        tracing::warn!(origin, "return_to origin not in CORS_ALLOWED_ORIGINS");
        return Err(ApiError::BadRequest);
    }

    Ok(trimmed.to_string())
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

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::{configured_server_admin_cids, validate_return_to};
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

        assert_eq!(configured_server_admin_cids(), vec![1234567]);
    }

    #[test]
    fn parses_multiple_configured_server_admin_cids() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("OSMIUM_SERVER_ADMIN_CID", "111, 222 ,333");

        assert_eq!(configured_server_admin_cids(), vec![111, 222, 333]);
    }

    #[test]
    fn ignores_invalid_configured_server_admin_cid() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("OSMIUM_SERVER_ADMIN_CID", "abc");

        assert!(configured_server_admin_cids().is_empty());
    }

    #[test]
    fn ignores_missing_configured_server_admin_cid() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::unset("OSMIUM_SERVER_ADMIN_CID");

        assert!(configured_server_admin_cids().is_empty());
    }

    #[test]
    fn server_admin_role_constant_is_stable() {
        assert_eq!(SERVER_ADMIN_ROLE, "SERVER_ADMIN");
    }

    #[test]
    fn return_to_accepts_allowlisted_origin() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("CORS_ALLOWED_ORIGINS", "https://vzdc.org");

        assert_eq!(
            validate_return_to("https://vzdc.org/profile/loa").unwrap(),
            "https://vzdc.org/profile/loa".to_string()
        );
    }

    #[test]
    fn return_to_rejects_non_allowlisted_origin() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("CORS_ALLOWED_ORIGINS", "https://vzdc.org");

        assert!(validate_return_to("https://evil.example/phish").is_err());
    }

    #[test]
    fn return_to_rejects_when_allowlist_unset() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::unset("CORS_ALLOWED_ORIGINS");

        assert!(validate_return_to("https://vzdc.org/").is_err());
    }

    #[test]
    fn return_to_rejects_non_http_scheme() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("CORS_ALLOWED_ORIGINS", "https://vzdc.org");

        assert!(validate_return_to("javascript:alert(1)").is_err());
    }

    #[test]
    fn return_to_rejects_empty_value() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _guard = EnvVarGuard::set("CORS_ALLOWED_ORIGINS", "https://vzdc.org");

        assert!(validate_return_to("   ").is_err());
    }
}
