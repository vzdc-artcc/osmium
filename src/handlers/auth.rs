use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Redirect,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{
        acl::{Permission, fetch_user_access},
        middleware::CurrentUser,
        middleware::ensure_permission,
        vatsim::{VatsimOAuthConfig, exchange_code_for_token, fetch_profile},
    },
    errors::ApiError,
    state::AppState,
};

const OAUTH_STATE_COOKIE: &str = "osmium_oauth_state";
const SESSION_COOKIE: &str = "osmium_session";
const OAUTH_STATE_TTL_SECS: i64 = 10 * 60;
const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 30;

#[derive(Deserialize)]
pub struct LoginQuery {
    prompt: Option<String>,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Serialize)]
pub struct SessionBody {
    id: String,
    cid: i64,
    email: String,
    display_name: String,
    role: String,
    roles: Vec<String>,
    permissions: Vec<Permission>,
}

pub async fn me(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
) -> Result<Json<SessionBody>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    ensure_permission(&state, Some(user), Permission::ReadOwnProfile).await?;

    let (roles, permissions) = fetch_user_access(state.db.as_ref(), &user.id, &user.role).await?;

    Ok(Json(SessionBody {
        id: user.id.clone(),
        cid: user.cid,
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        role: user.role.clone(),
        roles,
        permissions,
    }))
}

pub async fn vatsim_login(
    jar: CookieJar,
    Query(query): Query<LoginQuery>,
) -> Result<(CookieJar, Redirect), ApiError> {
    let config = VatsimOAuthConfig::from_env()?;
    let oauth_state = Uuid::new_v4().to_string();

    let mut authorize_url = Url::parse(&config.authorization_url(&oauth_state)?)
        .map_err(|_| ApiError::Internal)?;

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

    let user_id = sqlx::query_scalar::<_, String>(
        r#"
        insert into users (id, cid, email, display_name, role)
        values ($1, $2, $3, $4, 'USER')
        on conflict (cid) do update
        set email = excluded.email,
            display_name = excluded.display_name,
            updated_at = now()
        returning id
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(profile.cid)
    .bind(profile.email)
    .bind(profile.display_name)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, cid = profile.cid, "failed to upsert user during oauth callback");
        ApiError::Internal
    })?;

    sqlx::query(
        r#"
        insert into user_roles (user_id, role_name)
        values ($1, 'USER')
        on conflict (user_id, role_name) do nothing
        "#,
    )
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, user_id = user_id.as_str(), "failed to ensure default user role during oauth callback");
        ApiError::Internal
    })?;

    let session_token = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into sessions (session_token, user_id, expires_at)
        values ($1, $2, now() + interval '30 days')
        "#,
    )
    .bind(&session_token)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, user_id = user_id.as_str(), "failed to create session during oauth callback");
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
        insert into users (id, cid, email, display_name, role)
        values ($1, $2, $3, $4, 'USER')
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
        insert into user_roles (user_id, role_name)
        values ($1, 'USER')
        on conflict (user_id, role_name) do nothing
        "#,
    )
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, user_id = user_id.as_str(), "failed to ensure default user role during dev cid login");
        ApiError::Internal
    })?;

    let session_token = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into sessions (session_token, user_id, expires_at)
        values ($1, $2, now() + interval '30 days')
        "#,
    )
    .bind(&session_token)
    .bind(&user_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, user_id = user_id.as_str(), "failed to create session during dev cid login");
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

pub async fn logout(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(session_token): Extension<Option<String>>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), ApiError> {
    ensure_permission(&state, current_user.as_ref(), Permission::Logout).await?;

    if let (Some(pool), Some(token)) = (state.db.as_ref(), session_token.as_deref()) {
        sqlx::query("delete from sessions where session_token = $1")
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

fn cookie_secure() -> bool {
    std::env::var("COOKIE_SECURE")
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn api_dev_mode_enabled() -> bool {
    env_flag_enabled("API_DEV_MODE") || env_flag_enabled("VATSIM_DEV_MODE")
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

