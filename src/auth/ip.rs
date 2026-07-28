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

/// Resolves the client IP from proxy headers, preferring `X-Forwarded-For` then
/// `X-Real-IP`. Only returns a value that parses as a valid `IpAddr`; malformed
/// header content yields `None`.
///
/// The `X-Forwarded-For` value is read at the hop appended by our own outermost
/// trusted proxy (`TRUSTED_PROXY_COUNT` entries from the right), **not** the
/// leftmost hop. The leftmost entries are supplied by the client and are freely
/// spoofable; since this same value keys the rate limiter and is stored as the
/// audit/request-log IP, trusting them would let a caller bypass per-IP
/// throttling and forge the logged IP.
pub fn client_ip(headers: &HeaderMap) -> Option<String> {
    client_ip_with_trust(headers, crate::config::trusted_proxy_count())
}

fn client_ip_with_trust(headers: &HeaderMap, trusted_proxy_count: usize) -> Option<String> {
    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        let hops: Vec<&str> = value
            .split(',')
            .map(str::trim)
            .filter(|hop| !hop.is_empty())
            .collect();

        if !hops.is_empty() {
            // The rightmost `trusted_proxy_count` hops were appended by our own
            // trusted proxies; the real client is the hop immediately to their
            // left. If the chain is shorter than the configured trust depth,
            // fall back to the leftmost (oldest) hop.
            let index = hops.len().saturating_sub(trusted_proxy_count.max(1));
            let candidate = hops[index];
            if candidate.parse::<std::net::IpAddr>().is_ok() {
                return Some(candidate.to_string());
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
    use super::client_ip_with_trust;
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
    fn takes_hop_appended_by_the_trusted_proxy() {
        // Client → trusted proxy → app. The proxy appends the real client IP on
        // the right; the leftmost value is whatever the client claimed.
        let map = headers(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1")]);
        assert_eq!(client_ip_with_trust(&map, 1), Some("10.0.0.1".to_string()));
    }

    #[test]
    fn ignores_spoofed_leftmost_hop() {
        // A caller prepending a spoofed IP must not control the resolved value.
        let map = headers(&[("x-forwarded-for", "1.2.3.4, 198.51.100.9")]);
        assert_eq!(
            client_ip_with_trust(&map, 1),
            Some("198.51.100.9".to_string())
        );
    }

    #[test]
    fn honours_deeper_trusted_proxy_chains() {
        // Two trusted proxies each append one hop on the right, so with N=2 the
        // real client is at index len-2: here the middle entry. The leftmost
        // ("203.0.113.7") is the client-supplied, spoofable value and is ignored.
        let map = headers(&[("x-forwarded-for", "203.0.113.7, 192.0.2.1, 10.0.0.1")]);
        assert_eq!(client_ip_with_trust(&map, 2), Some("192.0.2.1".to_string()));
    }

    #[test]
    fn falls_back_to_leftmost_when_chain_shorter_than_trust_depth() {
        let map = headers(&[("x-forwarded-for", "203.0.113.7")]);
        assert_eq!(
            client_ip_with_trust(&map, 3),
            Some("203.0.113.7".to_string())
        );
    }

    #[test]
    fn falls_back_to_real_ip() {
        let map = headers(&[("x-real-ip", "198.51.100.23")]);
        assert_eq!(
            client_ip_with_trust(&map, 1),
            Some("198.51.100.23".to_string())
        );
    }

    #[test]
    fn rejects_invalid_forwarded_for_value() {
        // This is the case the old logging.rs helper silently accepted.
        let map = headers(&[("x-forwarded-for", "not-an-ip")]);
        assert_eq!(client_ip_with_trust(&map, 1), None);
    }

    #[test]
    fn rejects_invalid_real_ip_value() {
        let map = headers(&[("x-real-ip", "garbage")]);
        assert_eq!(client_ip_with_trust(&map, 1), None);
    }

    #[test]
    fn none_when_no_headers() {
        assert_eq!(client_ip_with_trust(&HeaderMap::new(), 1), None);
    }
}
