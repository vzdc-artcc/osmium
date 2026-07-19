use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("bad request")]
    BadRequest,
    #[error("oauth login origin mismatch")]
    OAuthLoginOriginMismatch,
    #[error("oauth state cookie missing")]
    OAuthStateCookieMissing,
    #[error("oauth state mismatch")]
    OAuthStateMismatch,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("service unavailable")]
    ServiceUnavailable,
    #[error("internal server error")]
    Internal,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::OAuthLoginOriginMismatch => {
                (StatusCode::BAD_REQUEST, "oauth_login_origin_mismatch")
            }
            Self::OAuthStateCookieMissing => {
                (StatusCode::BAD_REQUEST, "oauth_state_cookie_missing")
            }
            Self::OAuthStateMismatch => (StatusCode::BAD_REQUEST, "oauth_state_mismatch"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "conflict"),
            Self::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};

    use super::ApiError;

    #[tokio::test]
    async fn oauth_state_cookie_missing_maps_to_specific_bad_request() {
        let response = ApiError::OAuthStateCookieMissing.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            r#"{"error":"oauth_state_cookie_missing"}"#
        );
    }

    #[tokio::test]
    async fn oauth_state_mismatch_maps_to_specific_bad_request() {
        let response = ApiError::OAuthStateMismatch.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            r#"{"error":"oauth_state_mismatch"}"#
        );
    }

    #[tokio::test]
    async fn oauth_login_origin_mismatch_maps_to_specific_bad_request() {
        let response = ApiError::OAuthLoginOriginMismatch.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            r#"{"error":"oauth_login_origin_mismatch"}"#
        );
    }

    #[tokio::test]
    async fn forbidden_maps_to_403() {
        let response = ApiError::Forbidden.into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            r#"{"error":"forbidden"}"#
        );
    }

    #[tokio::test]
    async fn not_found_maps_to_404() {
        let response = ApiError::NotFound.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            r#"{"error":"not_found"}"#
        );
    }

    #[tokio::test]
    async fn conflict_maps_to_409() {
        let response = ApiError::Conflict.into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            r#"{"error":"conflict"}"#
        );
    }
}
