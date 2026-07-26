//! Unified client-IP extraction (spec 010).
//!
//! Before this module two near-duplicate `client_ip()` implementations existed:
//! `repos/audit.rs` (validated the parsed value) and `logging.rs` (did **not**
//! validate). This is now the single source of truth for both, plus the rate
//! limiter's key extraction — so the throttling key, the audit `ip_address`, and
//! the request log all agree on exactly which value a request's IP is.
//!
//! Spec 010 deliberately gives `logging.rs` the validation it previously lacked;
//! this is an intentional, called-out behavior change, not silent scope creep.

use http::HeaderMap;

/// Resolves the client IP from proxy headers, preferring `X-Forwarded-For`
/// (first hop) then `X-Real-IP`. Only returns a value that parses as a valid
/// `IpAddr`; malformed header content yields `None`.
pub fn client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        if let Some(first) = value.split(',').next() {
            let parsed = first.trim();
            if !parsed.is_empty() && parsed.parse::<std::net::IpAddr>().is_ok() {
                return Some(parsed.to_string());
            }
        }
    }

    if let Some(value) = headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
    {
        let parsed = value.trim();
        if !parsed.is_empty() && parsed.parse::<std::net::IpAddr>().is_ok() {
            return Some(parsed.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::client_ip;
    use http::HeaderMap;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    #[test]
    fn prefers_first_forwarded_for_hop() {
        let map = headers(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1")]);
        assert_eq!(client_ip(&map), Some("203.0.113.7".to_string()));
    }

    #[test]
    fn falls_back_to_real_ip() {
        let map = headers(&[("x-real-ip", "198.51.100.23")]);
        assert_eq!(client_ip(&map), Some("198.51.100.23".to_string()));
    }

    #[test]
    fn rejects_invalid_forwarded_for_value() {
        // This is the case the old logging.rs helper silently accepted.
        let map = headers(&[("x-forwarded-for", "not-an-ip")]);
        assert_eq!(client_ip(&map), None);
    }

    #[test]
    fn rejects_invalid_real_ip_value() {
        let map = headers(&[("x-real-ip", "garbage")]);
        assert_eq!(client_ip(&map), None);
    }

    #[test]
    fn none_when_no_headers() {
        assert_eq!(client_ip(&HeaderMap::new()), None);
    }
}
