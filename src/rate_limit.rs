//! Per-source-IP rate limiting with a permission-based bypass (spec 010).
//!
//! ## Why `governor` directly rather than `tower_governor`'s layer
//!
//! `tower_governor` ships an axum `GovernorLayer`, but it is a *synchronous* tower
//! layer: on an over-limit request it rejects immediately with a fixed response. The
//! spec requires that an over-limit request first get an **async, DB-backed**
//! permission check (`SystemRateLimitBypass`) and only be rejected if that fails —
//! and that the DB check runs *only* for already-over-limit traffic, never on the
//! hot path. That control flow can't be expressed through `GovernorLayer`, so this
//! module drives `governor`'s keyed limiter (the exact engine `tower_governor` wraps)
//! from a custom `from_fn` middleware instead.
//!
//! ## Hot-path cost
//!
//! An IP under its limit costs one in-memory `check_key` and **zero** DB round-trips.
//! The `ensure_permission` DB query happens only after an IP is already over its
//! limit — bounding that cost to abusive traffic, not normal reads.

use std::{num::NonZeroU32, sync::Arc};

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{DefaultKeyedRateLimiter, Quota};

use crate::{
    auth::{
        context::{CurrentServiceAccount, CurrentUser},
        ip::client_ip,
        middleware::ensure_permission,
        permissions::SystemRateLimitBypass,
        require_permission::Permission,
    },
    errors::ApiError,
    state::AppState,
};

/// Keyed by client-IP string, on the default (in-memory) store and clock.
pub type IpRateLimiter = DefaultKeyedRateLimiter<String>;

/// Shared bucket key for requests whose IP can't be resolved from proxy headers.
/// A single shared bucket (rather than skipping the limit) means header-stripping
/// can't be used to evade throttling — such traffic is collectively limited.
const UNKNOWN_IP_KEY: &str = "unknown";

/// Builds the process-wide limiter from configured per-minute / burst values.
/// Sustained rate = `requests_per_min` cells/minute; `burst` is the bucket size.
pub fn build_rate_limiter() -> Arc<IpRateLimiter> {
    let per_minute = NonZeroU32::new(crate::config::rate_limit_requests_per_min())
        .unwrap_or(NonZeroU32::new(1).expect("1 is non-zero"));
    let burst = NonZeroU32::new(crate::config::rate_limit_burst())
        .unwrap_or(NonZeroU32::new(1).expect("1 is non-zero"));

    let quota = Quota::per_minute(per_minute).allow_burst(burst);
    Arc::new(DefaultKeyedRateLimiter::keyed(quota))
}

/// Middleware enforcing the per-IP limit. Registered innermost (closest to the
/// handler) so `log_requests` still observes and logs any `429` — see
/// `router.rs` for the exact layer ordering and its rationale.
pub async fn enforce_rate_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !state.rate_limit_enabled {
        return next.run(request).await;
    }

    let key = client_ip(request.headers()).unwrap_or_else(|| UNKNOWN_IP_KEY.to_string());

    // In-memory check, no DB. Ok = under limit (a cell is consumed); Err = over.
    if state.rate_limiter.check_key(&key).is_ok() {
        return next.run(request).await;
    }

    // Over limit: only now pay for the bypass permission check. `resolve_current_user`
    // has already populated these extensions (it layers outside this middleware).
    let current_user = request
        .extensions()
        .get::<Option<CurrentUser>>()
        .and_then(Option::as_ref);
    let current_service_account = request
        .extensions()
        .get::<Option<CurrentServiceAccount>>()
        .and_then(Option::as_ref);

    match ensure_permission(
        &state,
        current_user,
        current_service_account,
        SystemRateLimitBypass::path(),
    )
    .await
    {
        // Bypass holder: allow through without counting this request against the
        // limiter (it was already over, so no cell was consumed above).
        Ok(()) => next.run(request).await,
        Err(_) => ApiError::TooManyRequests.into_response(),
    }
}
