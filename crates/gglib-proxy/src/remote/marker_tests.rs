//! Tests for [`super::Tunnelled::from_headers`] — reading the edge's markers.

use axum::http::{HeaderMap, HeaderValue};

use super::*;

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    map
}

#[test]
fn a_plain_request_is_not_tunnelled() {
    assert!(Tunnelled::from_headers(&headers(&[("host", "127.0.0.1:8080")])).is_none());
}

#[test]
fn the_edges_via_marks_the_request_and_names_the_peer() {
    let t = Tunnelled::from_headers(&headers(&[
        ("via", "1.1 modelpipe"),
        ("x-modelpipe-peer", "3CA82708B995"),
    ]))
    .expect("tunnelled");
    assert_eq!(t.peer.as_deref(), Some("3ca82708b995"), "lower-cased");
}

#[test]
fn a_via_from_some_other_proxy_does_not_count() {
    for via in ["1.1 nginx", "1.0 fred", "HTTP/1.1 modelpipe-ish"] {
        assert!(
            Tunnelled::from_headers(&headers(&[("via", via)])).is_none(),
            "{via:?}"
        );
    }
}

#[test]
fn the_pseudonym_is_found_in_a_chain_and_without_regard_to_case() {
    for via in [
        "1.1 a, 1.1 modelpipe",
        "1.1 MODELPIPE",
        "1.1 modelpipe (comment)",
    ] {
        assert!(
            Tunnelled::from_headers(&headers(&[("via", via)])).is_some(),
            "{via:?}"
        );
    }
}

#[test]
fn a_malformed_peer_is_dropped_but_the_request_is_still_tunnelled() {
    for peer in ["", "not-hex", "3ca8", "3ca82708b995aa"] {
        let t = Tunnelled::from_headers(&headers(&[
            ("via", "1.1 modelpipe"),
            ("x-modelpipe-peer", peer),
        ]))
        .expect("tunnelled");
        assert!(t.peer.is_none(), "{peer:?}");
    }
}

#[test]
fn the_peer_header_alone_marks_nothing() {
    assert!(
        Tunnelled::from_headers(&headers(&[("x-modelpipe-peer", "3ca82708b995")])).is_none(),
        "the Via is what says a request was tunnelled"
    );
}

/// The device name is read the way the edge writes it, and anything outside
/// modelpipe's own rule for a token name is dropped rather than kept.
///
/// Dropped and not truncated, because this value reaches a log line, a
/// settings row and a terminal. A truncated name would be a *different*
/// device's id with no way to tell, which is worse than none: the gate
/// refuses a request with no device, and refusing is always the safe answer
/// here.
#[test]
fn the_device_name_is_read_and_a_malformed_one_is_dropped() {
    let named = Tunnelled::from_headers(&headers(&[
        ("via", "1.1 modelpipe"),
        ("x-modelpipe-device", "dev-0a1b2c3d"),
    ]))
    .expect("tunnelled");
    assert_eq!(named.device.as_deref(), Some("dev-0a1b2c3d"));

    for bad in [
        "",
        "   ",
        "dev 0a1b2c3d",
        "dev/0a1b2c3d",
        "dev\u{e9}",
        &"d".repeat(65),
    ] {
        let read = Tunnelled::from_headers(&headers(&[
            ("via", "1.1 modelpipe"),
            ("x-modelpipe-device", bad),
        ]))
        .expect("still tunnelled");
        assert_eq!(read.device, None, "{bad:?} must not be kept");
    }

    // Sixty-four is inside the rule, so the bound is the right way round.
    let long = "d".repeat(64);
    let read = Tunnelled::from_headers(&headers(&[
        ("via", "1.1 modelpipe"),
        ("x-modelpipe-device", &long),
    ]))
    .expect("tunnelled");
    assert_eq!(read.device.as_deref(), Some(long.as_str()));
}

/// A device name without the `Via` is nothing at all, exactly as the peer
/// header alone is. The marker is what says a request crossed the tunnel;
/// the device says which key admitted it once it did.
#[test]
fn the_device_header_alone_marks_nothing() {
    assert!(Tunnelled::from_headers(&headers(&[("x-modelpipe-device", "dev-0a1b2c3d")])).is_none());
}
