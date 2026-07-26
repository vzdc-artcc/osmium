//! Impersonation write guard (spec 012, security checklist #6 + #7).
//!
//! While a session is impersonating another user, two classes of mutating request
//! are refused with `403`, centrally (so a new route can't forget the rule):
//!
//! - **Self-service writes** (#6): routes that change the acting user's own account
//!   — profile, TeamSpeak/Discord links, welcome/broadcast acks, bookings — so the
//!   impersonated victim's account can't be silently mutated through the session.
//! - **Admin/integration writes** (#7): every outbound side effect osmium can emit
//!   (VATUSA/roster writes, email sends, Discord posts, job runs) originates from a
//!   mutating `/api/v1/admin/*` route. Blocking admin writes wholesale — rather than
//!   maintaining a drift-prone enumerated allow/deny list of the specific
//!   outbound-producing routes — guarantees the impersonated session can't fire
//!   integrations as/through the target. Impersonation is for read-only support and
//!   debugging; a real admin change is made as yourself after `stop`.
//!
//! The impersonation control routes themselves (`/api/v1/admin/impersonate/*`) are
//! exempt so their own handler-level guards (nested/self/SERVER_ADMIN refusal, and
//! `stop`) apply. Reads (GET) are never blocked.

use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::Method;

use crate::{auth::context::CurrentUser, errors::ApiError};

/// Blocks the request with `403` when the current session is impersonating and the
/// request is a mutating call that must be refused during impersonation (see
/// [`is_blocked_while_impersonating`]).
pub async fn block_impersonated_self_service(request: Request, next: Next) -> Response {
    let impersonating = request
        .extensions()
        .get::<Option<CurrentUser>>()
        .and_then(Option::as_ref)
        .is_some_and(CurrentUser::is_impersonated);

    if impersonating && is_blocked_while_impersonating(request.method(), request.uri().path()) {
        return ApiError::Forbidden.into_response();
    }

    next.run(request).await
}

/// Whether `(method, path)` is a mutation that must be refused during impersonation.
/// Matches on the raw path (robust regardless of middleware layer position, which is
/// why it doesn't rely on `MatchedPath`).
pub fn is_blocked_while_impersonating(method: &Method, path: &str) -> bool {
    let mutating = matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if !mutating {
        return false;
    }

    // The impersonation control routes keep their own handler-level guards (nested
    // refusal, stop). Never intercept them here.
    if path.starts_with("/api/v1/admin/impersonate") {
        return false;
    }

    // #7 — every outbound side effect (VATUSA/roster, email, Discord, job runs)
    // originates from an admin/integration mutation.
    if path.starts_with("/api/v1/admin/") {
        return true;
    }

    is_self_service_write(path)
}

/// Self-service mutations (#6) that change the acting user's own account.
fn is_self_service_write(path: &str) -> bool {
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
    use super::is_blocked_while_impersonating;
    use http::Method;

    #[test]
    fn blocks_self_service_mutations() {
        assert!(is_blocked_while_impersonating(&Method::PATCH, "/api/v1/me"));
        assert!(is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/me/teamspeak-uids"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::DELETE,
            "/api/v1/me/teamspeak-uids/abc"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/me/discord/link/start"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/welcome-message/ack"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/broadcasts/xyz/agree"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/bookings"
        ));
    }

    #[test]
    fn blocks_admin_outbound_mutations() {
        // #7 — admin/integration writes are the outbound side-effect surface.
        assert!(is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/admin/notifications/announcements"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::PATCH,
            "/api/v1/admin/visitor-applications/abc"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::PATCH,
            "/api/v1/admin/users/123/flags"
        ));
        assert!(is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/admin/users/123/refresh-vatusa"
        ));
    }

    #[test]
    fn allows_reads_and_impersonation_control() {
        // Reads are always allowed.
        assert!(!is_blocked_while_impersonating(&Method::GET, "/api/v1/me"));
        assert!(!is_blocked_while_impersonating(
            &Method::GET,
            "/api/v1/admin/users/123/flags"
        ));
        // The stop-impersonation control must never be blocked.
        assert!(!is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/admin/impersonate/stop"
        ));
        // Start keeps its own handler guards (nested refusal), not blocked here.
        assert!(!is_blocked_while_impersonating(
            &Method::POST,
            "/api/v1/admin/impersonate/700099"
        ));
    }
}
