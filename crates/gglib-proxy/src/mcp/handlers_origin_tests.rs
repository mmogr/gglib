//! The tool gateway's own Origin check. Under `LocalOnly`, the config the
//! runtime serves the proxy with, the router's origin guard refuses the same
//! requests first, which is why its rules are checked here, on the function
//! itself. `tests/integration_origin.rs` serves a proxy under `AllowAll`,
//! where this check is the one that refuses a page on another site.

use axum::http::{HeaderMap, HeaderValue};

use super::validate_origin;

/// Headers carrying each `(name, value)`, the value as raw bytes.
fn carrying(pairs: &[(&'static str, &[u8])]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        headers.insert(*name, HeaderValue::from_bytes(value).unwrap());
    }
    headers
}

#[test]
fn validate_origin_allows_no_origin_header() {
    let headers = HeaderMap::new();
    assert!(validate_origin(&headers).is_ok());
}

#[test]
fn validate_origin_allows_localhost() {
    for origin in [
        "http://localhost",
        "http://localhost:3000",
        "https://localhost:8443",
        "http://127.0.0.1:9887",
        "https://127.0.0.1",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert("origin", HeaderValue::from_str(origin).unwrap());
        assert!(
            validate_origin(&headers).is_ok(),
            "expected {origin} to be allowed"
        );
    }
}

#[test]
fn validate_origin_rejects_external_origins() {
    for origin in [
        "https://evil.example.com",
        "http://attacker.io",
        "https://192.168.1.1:8080",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert("origin", HeaderValue::from_str(origin).unwrap());
        assert!(
            validate_origin(&headers).is_err(),
            "expected {origin} to be rejected"
        );
    }
}

/// No browser sends an `Origin` that is not text, but one that arrives is
/// refused. Read as absent, it would pass as a program's request.
#[test]
fn an_origin_that_is_not_text_is_refused_not_read_as_absent() {
    let origin: &[u8] = b"http://\xffevil.example";
    assert!(validate_origin(&carrying(&[("origin", origin)])).is_err());
}

/// Behind `--allowed-host gglib.lan` the proxy's own page names the host it
/// was sent to; sent to a host it does not name, it is another site.
#[test]
fn the_proxys_own_origin_passes_and_only_to_the_host_it_names() {
    let own: (&str, &[u8]) = ("origin", b"http://gglib.lan:8080");
    let named = carrying(&[own, ("host", b"gglib.lan:8080")]);
    assert!(validate_origin(&named).is_ok());
    let loopback = carrying(&[own, ("host", b"127.0.0.1:8080")]);
    assert!(validate_origin(&loopback).is_err());
}

#[test]
fn a_cross_site_request_without_an_origin_is_refused_by_its_fetch_metadata() {
    let cross_site = carrying(&[("sec-fetch-site", b"cross-site")]);
    assert!(validate_origin(&cross_site).is_err());
    let same_origin = carrying(&[("sec-fetch-site", b"same-origin")]);
    assert!(validate_origin(&same_origin).is_ok());
}
