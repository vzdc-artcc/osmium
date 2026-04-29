use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Redirect,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{
        acl::{
            PermissionAction, PermissionKey, PermissionResource, fetch_service_account_access,
            fetch_user_access, group_permission_keys,
        },
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
        vatsim::{VatsimOAuthConfig, exchange_code_for_token, fetch_profile},
    },
    errors::ApiError,
    models::ServiceAccountSessionBody,
    repos::access as access_repo,
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

#[derive(Serialize, ToSchema)]
pub struct SessionBody {
    id: String,
    cid: i64,
    email: String,
    display_name: String,
    rating: Option<String>,
    role: Option<String>,
    roles: Vec<String>,
    permissions: BTreeMap<String, Vec<String>>,
}

#[derive(sqlx::FromRow)]
struct OAuthUserUpsertRow {
    id: String,
    created_user: bool,
}

#[derive(sqlx::FromRow)]
struct OAuthMembershipUpsertRow {
    rating: Option<String>,
    created_membership: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current authenticated user session", body = SessionBody),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn me(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<SessionBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(
        &state,
        Some(user),
        None,
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Read),
    )
    .await?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;

    Ok(Json(SessionBody {
        id: user.id.clone(),
        cid: user.cid,
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        rating: user.rating.clone(),
        role: user.primary_role.clone(),
        roles,
        permissions: group_permission_keys(&permissions),
    }))
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
        permissions: group_permission_keys(&permissions),
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
    Query(query): Query<LoginQuery>,
) -> Result<(CookieJar, Redirect), ApiError> {
    let config = VatsimOAuthConfig::from_env()?;
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
        return Err(ApiError::BadRequest);
    };

    if cookie_state != callback_state {
        tracing::warn!("oauth callback state mismatch");
        return Err(ApiError::BadRequest);
    }

    let config = VatsimOAuthConfig::from_env()?;
    let access_token = exchange_code_for_token(&config, code).await?;
    let profile = fetch_profile(&config, &access_token).await?;

    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let user_upsert = sqlx::query_as::<_, OAuthUserUpsertRow>(
        r#"
        insert into identity.users (id, cid, email, full_name, display_name)
        values ($1, $2, $3, $4, $4)
        on conflict (cid) do update
        set email = excluded.email,
            full_name = excluded.full_name,
            display_name = excluded.display_name,
            updated_at = now()
        returning id, (xmax = 0) as created_user
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(profile.cid)
    .bind(profile.email)
    .bind(profile.display_name)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!(
            ?error,
            cid = profile.cid,
            "failed to upsert user during oauth callback"
        );
        ApiError::Internal
    })?;
    let user_id = user_upsert.id;

    let membership_upsert = sqlx::query_as::<_, OAuthMembershipUpsertRow>(
        r#"
        insert into org.memberships (
            user_id,
            artcc,
            division,
            rating,
            membership_status,
            controller_status,
            updated_at
        )
        values ($1, 'ZDC', 'USA', $2, 'ACTIVE', 'NONE', now())
        on conflict (user_id) do update
        set rating = coalesce(excluded.rating, org.memberships.rating),
            membership_status = 'ACTIVE',
            updated_at = now()
        returning rating, (xmax = 0) as created_membership
        "#,
    )
    .bind(&user_id)
    .bind(profile.rating.as_deref())
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!(
            ?error,
            cid = profile.cid,
            user_id = user_id.as_str(),
            "failed to upsert membership during oauth callback"
        );
        ApiError::Internal
    })?;

    tracing::info!(
        cid = profile.cid,
        user_id = user_id.as_str(),
        source = "vatsim_oauth",
        rating = membership_upsert.rating.as_deref(),
        created_user = user_upsert.created_user,
        created_membership = membership_upsert.created_membership,
        membership_synced = true,
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

    let user_id = sqlx::query_scalar::<_, String>(
        r#"
        insert into identity.users (id, cid, email, full_name, display_name)
        values ($1, $2, $3, $4, $4)
        on conflict (cid) do update
        set updated_at = now()
        returning id
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(cid)
    .bind(generated_email)
    .bind(generated_name)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, cid, "failed to upsert user during dev cid login");
        ApiError::Internal
    })?;

    sqlx::query(
        r#"
        insert into org.memberships (user_id, artcc, division, membership_status, controller_status)
        values ($1, 'ZDC', 'USA', 'ACTIVE', 'NONE')
        on conflict (user_id) do nothing
        "#,
    )
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

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
        PermissionKey::new(PermissionResource::Auth, PermissionAction::Delete),
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
            sqlx::query(
                r#"
                insert into access.user_roles (user_id, role_name)
                values ($1, 'USER')
                on conflict (user_id, role_name) do nothing
                "#,
            )
            .bind(user_id)
            .execute(pool)
            .await
            .map_err(|_| ApiError::Internal)?;

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
