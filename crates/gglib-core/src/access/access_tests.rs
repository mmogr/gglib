//! Tests for the access module's own items: the constant-time compare, the
//! minted credentials, and [`ProxyAccessConfig`]'s allowlist. The Host
//! predicate has its own file beside it.

use super::*;

/// Six decimal digits, every time, including when the draw lands below
/// 100000 and needs its zeros.
#[test]
fn a_pairing_code_is_six_decimal_digits() {
    for _ in 0..256 {
        let code = generate_pairing_code();
        assert_eq!(code.len(), 6, "{code}");
        assert!(code.bytes().all(|b| b.is_ascii_digit()), "{code}");
    }
}

/// The proxy's superset of the two former private copies: it also covers the
/// empty-vs-nonempty case, which the axum copy did not.
#[test]
fn constant_time_eq_agrees_with_equality() {
    assert!(constant_time_eq(b"Bearer abc", b"Bearer abc"));
    assert!(constant_time_eq(b"", b""));
    assert!(!constant_time_eq(b"Bearer abc", b"Bearer abd"));
    assert!(!constant_time_eq(b"Bearer abc", b"Bearer ab"));
    assert!(!constant_time_eq(b"", b"x"));
}

/// A prefix match must not pass — the failure mode a naive `starts_with`
/// would introduce.
#[test]
fn constant_time_eq_rejects_a_prefix() {
    assert!(!constant_time_eq(b"Bearer secret", b"Bearer secret-extra"));
}

/// The guards read the bare token off this field, so its presence is what
/// decides whether authentication is enforced at all.
#[test]
fn the_api_key_is_carried_verbatim() {
    let off = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "127.0.0.1", vec![]);
    assert_eq!(off.api_key, None);

    let on = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some("secret123".into()),
        "127.0.0.1",
        vec![],
    );
    assert_eq!(on.api_key.as_deref(), Some("secret123"));
}

/// Loopback needs no configuration — the common case must not require a
/// flag.
#[test]
fn loopback_bind_allows_the_usual_local_names() {
    let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "127.0.0.1", vec![]);
    assert!(cfg.allowed_hosts.is_empty());
    for host in [
        "127.0.0.1:8080",
        "localhost:8080",
        "LOCALHOST",
        "[::1]:8080",
        "127.0.0.2",
    ] {
        assert!(cfg.host_allowed(host), "{host} should be allowed");
    }
}

/// The whole point: a rebound page asks for its own hostname.
#[test]
fn a_foreign_host_is_rejected() {
    let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "127.0.0.1", vec![]);
    assert!(!cfg.host_allowed("evil.com"));
    assert!(!cfg.host_allowed("evil.com:8080"));
    assert!(!cfg.host_allowed(""));
}

/// Binding a concrete LAN address is itself the statement that the address
/// is meant to be reachable.
#[test]
fn a_concrete_bind_host_allows_itself() {
    let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "192.168.1.5", vec![]);
    assert_eq!(cfg.allowed_hosts, vec!["192.168.1.5".to_string()]);
    assert!(cfg.host_allowed("192.168.1.5:8080"));
    assert!(cfg.host_allowed("127.0.0.1:8080"));
    assert!(!cfg.host_allowed("evil.com"));
}

/// A wildcard names no address, so it grants nothing. This is the
/// breaking case for existing `--host 0.0.0.0` users, and it is deliberate.
#[test]
fn a_wildcard_bind_grants_nothing_on_its_own() {
    let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "0.0.0.0", vec![]);
    assert!(cfg.allowed_hosts.is_empty());
    assert!(!cfg.host_allowed("192.168.1.5:8080"));
    assert!(cfg.host_allowed("127.0.0.1:8080"));
}

#[test]
fn explicit_allowed_hosts_are_honoured_under_a_wildcard_bind() {
    let cfg = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        None,
        "0.0.0.0",
        vec!["gglib.lan".into(), "192.168.1.5:8080".into()],
    );
    assert!(cfg.host_allowed("gglib.lan"));
    assert!(cfg.host_allowed("GGLIB.LAN:8080"));
    assert!(cfg.host_allowed("192.168.1.5:9999"));
}

/// The bind host must not be listed twice when it is also passed
/// explicitly.
#[test]
fn duplicate_entries_collapse() {
    let cfg = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        None,
        "192.168.1.5",
        vec!["192.168.1.5".into(), "192.168.1.5:8080".into()],
    );
    assert_eq!(cfg.allowed_hosts, vec!["192.168.1.5".to_string()]);
}
