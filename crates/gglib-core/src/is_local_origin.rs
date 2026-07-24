//! Origin validation utilities for CORS and similar security checks.
//!
//! Provides [`is_local_origin`] to determine whether a URL origin
//! (e.g. from the `Origin` HTTP header) is a trusted local address.

use url::Url;

/// Returns `true` if the origin string resolves to a local host.
///
/// Accepted hosts: `localhost`, `127.0.0.1`, `::1`.
///
/// Schemes `http` and `https` are accepted; ports are ignored.
/// Malformed URLs, missing hosts, and non-local hosts return `false`.
pub fn is_local_origin(origin: &str) -> bool {
    let Ok(parsed) = Url::parse(origin) else {
        return false;
    };

    // Only allow http/https schemes
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }

    let Some(host_str) = parsed.host_str() else {
        return false;
    };

    // `url::Url::host_str()` returns IPv6 addresses with brackets (e.g. `[::1]`),
    // so strip them for comparison.
    let host = host_str.trim_start_matches('[').trim_end_matches(']');

    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_localhost_http() {
        assert!(is_local_origin("http://localhost"));
    }

    #[test]
    fn accepts_localhost_https() {
        assert!(is_local_origin("https://localhost"));
    }

    #[test]
    fn accepts_localhost_with_port() {
        assert!(is_local_origin("http://localhost:3000"));
        assert!(is_local_origin("https://localhost:8080"));
    }

    #[test]
    fn accepts_127_0_0_1() {
        assert!(is_local_origin("http://127.0.0.1"));
        assert!(is_local_origin("https://127.0.0.1"));
        assert!(is_local_origin("http://127.0.0.1:9887"));
    }

    #[test]
    fn accepts_ipv6_loopback() {
        assert!(is_local_origin("http://[::1]"));
        assert!(is_local_origin("https://[::1]"));
        assert!(is_local_origin("http://[::1]:3000"));
    }

    #[test]
    fn rejects_subdomain_of_localhost() {
        assert!(!is_local_origin("http://localhost.evil.com"));
    }

    #[test]
    fn rejects_notlocalhost() {
        assert!(!is_local_origin("http://notlocalhost"));
    }

    #[test]
    fn rejects_non_loopback_ip() {
        assert!(!is_local_origin("http://127.0.0.2"));
    }

    #[test]
    fn rejects_external_host() {
        assert!(!is_local_origin("https://example.com"));
    }

    #[test]
    fn rejects_non_loopback_ipv6() {
        assert!(!is_local_origin("http://[::2]"));
    }

    #[test]
    fn rejects_empty_string() {
        assert!(!is_local_origin(""));
    }

    #[test]
    fn rejects_malformed_url() {
        assert!(!is_local_origin("not-a-url"));
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(!is_local_origin("ftp://localhost"));
    }

    #[test]
    fn rejects_url_encoded_bypass() {
        // `http://localhost:8080@evil.com` parses with host = "evil.com"
        assert!(!is_local_origin("http://localhost:8080@evil.com"));
    }
}
