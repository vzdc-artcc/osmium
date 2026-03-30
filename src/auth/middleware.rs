use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::acl::{Permission, fetch_user_access},
    errors::ApiError,
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CurrentUser {
    pub id: String,
    pub cid: i64,
    pub email: String,
    pub display_name: String,
    pub role: String,
}

pub async fn resolve_current_user(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let session_token = parse_cookie(
        request.headers().get(http::header::COOKIE),
        "osmium_session",
    );

    let current_user =
        if let (Some(pool), Some(token)) = (state.db.as_ref(), session_token.as_deref()) {
            sqlx::query_as::<_, CurrentUser>(
                r#"
            select u.id, u.cid, u.email, u.display_name, u.role
            from sessions s
            join users u on u.id = s.user_id
            where s.session_token = $1 and s.expires_at > now()
            "#,
            )
            .bind(token)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
        } else {
            None
        };

    request.extensions_mut().insert(current_user);
    request.extensions_mut().insert(session_token);

    next.run(request).await
}

pub async fn ensure_permission(
    state: &AppState,
    current_user: Option<&CurrentUser>,
    permission: Permission,
) -> Result<(), ApiError> {
    let Some(user) = current_user else {
        return Err(ApiError::Unauthorized);
    };

    let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id, &user.role).await?;

    if permissions.contains(&permission) {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

pub async fn require_staff(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let current_user = request.extensions().get::<Option<CurrentUser>>().cloned().flatten();

    if ensure_permission(&state, current_user.as_ref(), Permission::ManageUsers)
        .await
        .is_err()
    {
        return ApiError::Unauthorized.into_response();
    }

    next.run(request).await
}

fn parse_cookie(cookie_header: Option<&http::HeaderValue>, cookie_name: &str) -> Option<String> {
    let header_value = cookie_header?.to_str().ok()?;

    for raw_cookie in header_value.split(';') {
        let mut parts = raw_cookie.trim().splitn(2, '=');
        let name = parts.next()?.trim();
        let value = parts.next()?.trim();

        if name == cookie_name {
            return Some(value.to_string());
        }
    }

    None
}
