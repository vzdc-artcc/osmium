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

pub fn dev_impersonation_enabled() -> bool {
    env_flag_enabled("DEV_LOGIN_AS_CID_ENABLED")
}

pub fn dev_seed_enabled() -> bool {
    env_flag_enabled("DEV_SEED_ENABLED")
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
    use super::{dev_impersonation_enabled, dev_seed_enabled, normalize_origin};

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
    fn explicit_dev_login_flag_enables_impersonation_only() {
        let _login = EnvVarGuard::set("DEV_LOGIN_AS_CID_ENABLED", "true");
        let _seed = EnvVarGuard::set("DEV_SEED_ENABLED", "false");

        assert!(dev_impersonation_enabled());
        assert!(!dev_seed_enabled());
    }
}
