use std::net::SocketAddr;

use super::host::{
    endpoint_base_url, is_loopback_host, is_wildcard_host, normalize_host, reachable_at,
};

#[test]
fn normalizes_the_three_spellings_of_one_address_to_one_value() {
    assert_eq!(normalize_host("[::1]:8080").as_deref(), Some("::1"));
    assert_eq!(normalize_host("[::1]").as_deref(), Some("::1"));
    assert_eq!(normalize_host("::1").as_deref(), Some("::1"));
}

#[test]
fn strips_the_port_and_lowercases() {
    assert_eq!(
        normalize_host("GGLIB.Lan:8080").as_deref(),
        Some("gglib.lan")
    );
    assert_eq!(
        normalize_host("127.0.0.1:8080").as_deref(),
        Some("127.0.0.1")
    );
    assert_eq!(normalize_host(" localhost ").as_deref(), Some("localhost"));
}

/// Userinfo and paths make a `Host` malformed; a malformed claim is no claim.
#[test]
fn rejects_anything_that_cannot_be_a_host() {
    assert_eq!(normalize_host(""), None);
    assert_eq!(normalize_host("   "), None);
    assert_eq!(normalize_host("foo@localhost"), None);
    assert_eq!(normalize_host("localhost/../evil"), None);
    assert_eq!(normalize_host(":8080"), None);
    assert_eq!(normalize_host("[::1"), None);
    assert_eq!(normalize_host("[]"), None);
    assert_eq!(normalize_host("[::1]x"), None);
}

#[test]
fn loopback_covers_the_whole_127_block_and_ipv6() {
    for host in [
        "127.0.0.1",
        "127.0.0.1:8080",
        "127.0.0.2",
        "localhost",
        "LOCALHOST",
        "::1",
        "[::1]:8080",
    ] {
        assert!(is_loopback_host(host), "{host} should be loopback");
    }
}

#[test]
fn loopback_rejects_routable_and_malformed_hosts() {
    for host in [
        "evil.com",
        "192.168.1.5",
        "0.0.0.0",
        "",
        "localhost.evil.com",
    ] {
        assert!(!is_loopback_host(host), "{host} should not be loopback");
    }
}

#[test]
fn wildcard_covers_both_families() {
    assert!(is_wildcard_host("0.0.0.0"));
    assert!(is_wildcard_host("::"));
    assert!(is_wildcard_host("[::]:8080"));
    assert!(!is_wildcard_host("127.0.0.1"));
    assert!(!is_wildcard_host("192.168.1.5"));
    assert!(!is_wildcard_host("gglib.lan"));
}

/// Only a wildcard bind is rewritten: it names no reachable address, so a
/// client on this machine is sent to loopback. Every other bind is reached
/// where it is, and the base URL is that address under `/v1`.
#[test]
fn only_a_wildcard_bind_is_reached_on_loopback() {
    let cases = [
        ("0.0.0.0:8080", "127.0.0.1:8080"),
        ("[::]:8080", "127.0.0.1:8080"),
        ("127.0.0.1:8080", "127.0.0.1:8080"),
        ("[::1]:8080", "[::1]:8080"),
        ("192.168.1.20:8080", "192.168.1.20:8080"),
    ];
    for (bound, reached) in cases {
        let bound: SocketAddr = bound.parse().unwrap();
        assert_eq!(reachable_at(bound), reached.parse().unwrap(), "{bound}");
        assert_eq!(
            endpoint_base_url(bound),
            format!("http://{reached}/v1"),
            "{bound}"
        );
    }
}
