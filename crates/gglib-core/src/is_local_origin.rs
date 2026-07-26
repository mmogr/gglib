//! Origin validation utilities for CORS and similar security checks.
//!
//! Provides [`is_local_origin`] to determine whether a URL origin
//! (e.g. from the `Origin` HTTP header) is a trusted local address.

use url::Url;

/// Returns `true` if the origin string resolves to a local host.
///
/// Accepted hosts: `localhost`, `127.0.0.1`, `::1`, `tauri.localhost`.
///
/// Schemes `http` and `https` are accepted; ports are ignored.
/// Tauri custom schemes (`tauri://localhost`, `asset://localhost`) are also accepted.
/// URLs with userinfo (e.g. `http://user@localhost`) are rejected to prevent
/// credential-injection bypasses.
/// Malformed URLs, missing hosts, and non-local hosts return `false`.
pub fn is_local_origin(origin: &str) -> bool {
    // Handle Tauri custom schemes that Url::parse cannot parse (not registered URI schemes).
    if let Some(stripped) = origin
        .strip_prefix("tauri://")
        .or_else(|| origin.strip_prefix("asset://"))
    {
        let host = stripped.trim_end_matches('/');
        // Reject userinfo in custom schemes (defensive symmetry with the
        // standard URL userinfo guard below).
        if host.contains('@') {
            return false;
        }
        return matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    }

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

    // Reject URLs with userinfo (e.g. http://user@localhost) to prevent
    // credential-injection bypasses.
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }

    // `url::Url::host_str()` returns IPv6 addresses with brackets (e.g. `[::1]`),
    // so strip them for comparison.
    let host = host_str.trim_start_matches('[').trim_end_matches(']');

    // RFC 3986 §3.2.2: host comparison is case-insensitive for IPv4 and
    // domain names. Normalizing to lowercase documents this intent.
    matches!(
        host.to_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1" | "tauri.localhost"
    )
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

    #[test]
    fn accepts_tauri_scheme_localhost() {
        assert!(is_local_origin("tauri://localhost"));
        assert!(is_local_origin("tauri://localhost/"));
    }

    #[test]
    fn accepts_asset_scheme_localhost() {
        assert!(is_local_origin("asset://localhost"));
    }

    #[test]
    fn accepts_http_tauri_localhost() {
        assert!(is_local_origin("http://tauri.localhost"));
        assert!(is_local_origin("http://tauri.localhost:3000"));
    }

    #[test]
    fn rejects_userinfo_localhost() {
        // http://user@localhost parses with host="localhost" — must be rejected via userinfo guard
        assert!(!is_local_origin("http://user@localhost"));
    }

    #[test]
    fn rejects_userinfo_with_password() {
        assert!(!is_local_origin("http://user:pass@localhost"));
    }

    #[test]
    fn rejects_tauri_scheme_userinfo() {
        assert!(!is_local_origin("tauri://user@localhost"));
    }

    #[test]
    fn rejects_asset_scheme_external_host() {
        assert!(!is_local_origin("asset://evil.com"));
    }

    #[test]
    fn accepts_https_tauri_localhost() {
        assert!(is_local_origin("https://tauri.localhost"));
    }

    #[test]
    fn accepts_uppercase_localhost() {
        assert!(is_local_origin("http://LOCALHOST"));
    }
}
