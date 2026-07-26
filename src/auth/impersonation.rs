//! Impersonation self-service write guard (spec 012, security checklist #6).
//!
//! While a session is impersonating another user, mutating *self-service* routes
//! (the ones that change the acting user's own account) must be blocked so the
//! impersonated victim's profile, links, preferences and bookings can't be silently
//! mutated through the impersonation session. This is enforced centrally here rather
//! than in each handler, so it can't be forgotten on a new self-service route.
//!
//! Admin/staff routes are intentionally NOT blocked: an impersonation of a staff
//! member is for support/debugging, and those routes are still individually
//! permission-gated against the (impersonated) subject's own access.

use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::Method;

use crate::{auth::context::CurrentUser, errors::ApiError};

/// Blocks the request with `403` when the current session is impersonating and the
/// request is a mutating call to a self-service route (see [`is_self_service_write`]).
pub async fn block_impersonated_self_service(request: Request, next: Next) -> Response {
    let impersonating = request
        .extensions()
        .get::<Option<CurrentUser>>()
        .and_then(Option::as_ref)
        .is_some_and(CurrentUser::is_impersonated);

    if impersonating && is_self_service_write(request.method(), request.uri().path()) {
        return ApiError::Forbidden.into_response();
    }

    next.run(request).await
}

/// Whether `(method, path)` is a self-service mutation that must be refused during
/// impersonation. Matches on the raw path (robust regardless of middleware layer
/// position, which is why it doesn't rely on `MatchedPath`).
pub fn is_self_service_write(method: &Method, path: &str) -> bool {
    let mutating = matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if !mutating {
        return false;
    }

    // Self profile.
    if path == "/api/v1/me" {
        return true;
    }
    // Self TeamSpeak UID linkage and Discord link/unlink.
    if path.starts_with("/api/v1/me/teamspeak-uids") || path.starts_with("/api/v1/me/discord") {
        return true;
    }
    // Welcome-message acknowledgement.
    if path == "/api/v1/welcome-message/ack" {
        return true;
    }
    // Broadcast seen/agree acknowledgements.
    if path.starts_with("/api/v1/broadcasts/") && (path.ends_with("/seen") || path.ends_with("/agree"))
    {
        return true;
    }
    // ATC bookings (self bookings + an outbound VATSIM side effect).
    if path == "/api/v1/bookings" || path.starts_with("/api/v1/bookings/") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::is_self_service_write;
    use http::Method;

    #[test]
    fn blocks_self_service_mutations() {
        assert!(is_self_service_write(&Method::PATCH, "/api/v1/me"));
        assert!(is_self_service_write(
            &Method::POST,
            "/api/v1/me/teamspeak-uids"
        ));
        assert!(is_self_service_write(
            &Method::DELETE,
            "/api/v1/me/teamspeak-uids/abc"
        ));
        assert!(is_self_service_write(
            &Method::POST,
            "/api/v1/me/discord/link/start"
        ));
        assert!(is_self_service_write(
            &Method::POST,
            "/api/v1/welcome-message/ack"
        ));
        assert!(is_self_service_write(
            &Method::POST,
            "/api/v1/broadcasts/xyz/agree"
        ));
        assert!(is_self_service_write(&Method::POST, "/api/v1/bookings"));
    }

    #[test]
    fn allows_reads_and_non_self_service() {
        // Reads are always allowed.
        assert!(!is_self_service_write(&Method::GET, "/api/v1/me"));
        // The stop-impersonation control must never be blocked.
        assert!(!is_self_service_write(
            &Method::POST,
            "/api/v1/admin/impersonate/stop"
        ));
        // Admin routes are gated on the subject's own access, not blocked here.
        assert!(!is_self_service_write(
            &Method::PATCH,
            "/api/v1/admin/users/123/flags"
        ));
    }
}
