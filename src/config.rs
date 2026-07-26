use http::{
    HeaderValue, Method,
    header::{self, HeaderName},
};
use tower_http::cors::CorsLayer;

pub fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn vatsim_dev_mode_enabled() -> bool {
    env_flag_enabled("VATSIM_DEV_MODE")
}

pub fn dev_seed_enabled() -> bool {
    env_flag_enabled("DEV_SEED_ENABLED")
}

/// TTL for an impersonation session (spec 012). Deliberately much shorter than a
/// normal 30-day login so a forgotten "act as" doesn't linger. On stop the session
/// is restored to a normal login TTL.
pub fn impersonation_ttl_secs() -> i64 {
    std::env::var("IMPERSONATION_TTL_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(60 * 60)
}

/// Whether a boolean env var is enabled, defaulting to `true` when unset or
/// unparseable (used for feature flags that should be on unless explicitly
/// disabled, unlike [`env_flag_enabled`] which defaults off).
fn env_flag_enabled_default_true(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

fn env_u32_or(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// IP rate limiting is on unless `RATE_LIMIT_ENABLED` is explicitly falsey
/// (spec 010). The test harness sets it to `false` so unrelated tests don't trip
/// the limiter.
pub fn rate_limit_enabled() -> bool {
    env_flag_enabled_default_true("RATE_LIMIT_ENABLED")
}

/// Sustained requests allowed per source IP per minute (governor replenishment
/// rate). Generous default so SPA/NAT clients aren't falsely throttled while a
/// scraping/brute-force flood still trips it.
pub fn rate_limit_requests_per_min() -> u32 {
    env_u32_or("RATE_LIMIT_REQUESTS_PER_MIN", 1200)
}

/// Burst capacity per source IP (governor cell/bucket size) — how many requests
/// may arrive back-to-back before the per-minute rate begins to apply.
pub fn rate_limit_burst() -> u32 {
    env_u32_or("RATE_LIMIT_BURST", 240)
}

/// Dedicated tight limit for the GDPR data export (`GET /me/data-export`), keyed
/// per user. The export is one of the most expensive requests in the API (a full
/// cross-domain assembly), so the loose global per-IP limit is not enough — this
/// caps how often any one user can trigger it. Only enforced when `RATE_LIMIT_ENABLED`.
pub fn data_export_rate_limit_per_hour() -> u32 {
    env_u32_or("DATA_EXPORT_RATE_LIMIT_PER_HOUR", 12)
}

/// Burst for the per-user data-export limit — how many back-to-back exports before
/// the hourly rate applies (stops rapid button-spam while allowing a couple of
/// legitimate re-downloads).
pub fn data_export_rate_limit_burst() -> u32 {
    env_u32_or("DATA_EXPORT_RATE_LIMIT_BURST", 3)
}

// ---------------------------------------------------------------------------
// spec 011 — durable IP request tracking
// ---------------------------------------------------------------------------

/// Whether per-request IP metadata is buffered and persisted. On unless explicitly
/// falsey; the test harness disables it so unrelated tests don't produce log rows.
pub fn ip_request_log_enabled() -> bool {
    env_flag_enabled_default_true("IP_REQUEST_LOG_ENABLED")
}

/// Rows older than this are pruned by the cleanup job.
pub fn ip_request_log_retention_days() -> i64 {
    std::env::var("IP_REQUEST_LOG_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(30)
}

/// How often the drain job flushes buffered entries to the database.
pub fn ip_request_log_flush_secs() -> u64 {
    std::env::var("IP_REQUEST_LOG_FLUSH_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(5)
}

/// Maximum rows drained + inserted per flush tick.
pub fn ip_request_log_batch_size() -> usize {
    env_u32_or("IP_REQUEST_LOG_BATCH_SIZE", 500) as usize
}

/// Bounded channel capacity between the request middleware and the drain job.
/// When full, new entries are dropped (with a warning) rather than back-pressuring
/// real traffic.
pub fn ip_request_log_channel_capacity() -> usize {
    env_u32_or("IP_REQUEST_LOG_CHANNEL_CAPACITY", 10_000) as usize
}

/// Origins trusted for credentialed cross-origin requests. Reused as the
/// allowlist for OAuth `return_to` redirect targets (`src/handlers/auth.rs`)
/// since it's the same trust boundary: frontends we already trust to make
/// authenticated calls are the frontends we trust to redirect a login back to.
pub fn configured_allowed_origins() -> Vec<String> {
    let Some(raw) = std::env::var("CORS_ALLOWED_ORIGINS")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Vec::new();
    };

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_origin)
        .collect()
}

pub fn build_cors_layer() -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::ACCEPT,
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-requested-with"),
        ]);

    let origins = configured_cors_origins();
    if origins.is_empty() {
        layer
    } else {
        layer.allow_origin(origins)
    }
}

fn configured_cors_origins() -> Vec<HeaderValue> {
    configured_allowed_origins()
        .into_iter()
        .map(|origin| {
            HeaderValue::from_str(&origin)
                .unwrap_or_else(|_| panic!("invalid CORS origin header value: {origin}"))
        })
        .collect()
}

fn normalize_origin(raw: &str) -> String {
    let url = reqwest::Url::parse(raw).unwrap_or_else(|_| panic!("invalid CORS origin URL: {raw}"));
    let host = url
        .host_str()
        .unwrap_or_else(|| panic!("CORS origin is missing a host: {raw}"));
    let scheme = url.scheme();
    let port = url
        .port_or_known_default()
        .unwrap_or_else(|| panic!("CORS origin is missing a known port: {raw}"));

    let is_default_port = (scheme == "http" && port == 80) || (scheme == "https" && port == 443);
    if is_default_port {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::{dev_seed_enabled, normalize_origin};

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

    #[test]
    fn normalizes_default_port_origin() {
        assert_eq!(
            normalize_origin("https://app.example.org:443/path?q=1"),
            "https://app.example.org"
        );
    }

    #[test]
    fn normalizes_custom_port_origin() {
        assert_eq!(
            normalize_origin("http://127.0.0.1:5173/login"),
            "http://127.0.0.1:5173"
        );
    }

    #[test]
    fn dev_seed_flag_parses_independently() {
        let _seed = EnvVarGuard::set("DEV_SEED_ENABLED", "true");
        assert!(dev_seed_enabled());
    }
}
