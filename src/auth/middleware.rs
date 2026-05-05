use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::{
    auth::{
        acl::{PermissionPath, fetch_service_account_access, fetch_user_access},
        context::{CurrentServiceAccount, CurrentUser},
    },
    errors::ApiError,
    repos::access as access_repo,
    state::AppState,
};

pub async fn resolve_current_user(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let session_token = parse_cookie(
        request.headers().get(http::header::COOKIE),
        "osmium_session",
    );
    let bearer_token = parse_bearer_token(request.headers().get(http::header::AUTHORIZATION));

    let current_user =
        if let (Some(pool), Some(token)) = (state.db.as_ref(), session_token.as_deref()) {
            access_repo::find_current_user_by_session_token(pool, token)
                .await
                .ok()
                .flatten()
        } else {
            None
        };

    let current_service_account =
        if let (Some(pool), Some(token)) = (state.db.as_ref(), bearer_token.as_deref()) {
            access_repo::find_current_service_account_by_bearer_token(pool, token)
                .await
                .ok()
                .flatten()
        } else {
            None
        };

    request.extensions_mut().insert(current_user);
    request.extensions_mut().insert(current_service_account);
    request.extensions_mut().insert(session_token);
    request.extensions_mut().insert(bearer_token);

    next.run(request).await
}

pub async fn ensure_permission(
    state: &AppState,
    current_user: Option<&CurrentUser>,
    current_service_account: Option<&CurrentServiceAccount>,
    permission: PermissionPath,
) -> Result<(), ApiError> {
    if let Some(user) = current_user {
        let (_, permissions) = fetch_user_access(state.db.as_ref(), &user.id).await?;
        return if permissions.contains(&permission) {
            Ok(())
        } else {
            Err(ApiError::Unauthorized)
        };
    }

    if let Some(service_account) = current_service_account {
        let (_, permissions) =
            fetch_service_account_access(state.db.as_ref(), &service_account.id).await?;
        return if permissions.contains(&permission) {
            Ok(())
        } else {
            Err(ApiError::Unauthorized)
        };
    }

    Err(ApiError::Unauthorized)
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

fn parse_bearer_token(auth_header: Option<&http::HeaderValue>) -> Option<String> {
    let header_value = auth_header?.to_str().ok()?.trim();
    let token = header_value.strip_prefix("Bearer ")?;
    let token = token.trim();

    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_bearer_token;

    #[test]
    fn parses_bearer_token() {
        let value = http::HeaderValue::from_static("Bearer token-123");
        assert_eq!(
            parse_bearer_token(Some(&value)).as_deref(),
            Some("token-123")
        );
    }

    #[test]
    fn ignores_non_bearer_authorization() {
        let value = http::HeaderValue::from_static("Basic abc");
        assert!(parse_bearer_token(Some(&value)).is_none());
    }
}
