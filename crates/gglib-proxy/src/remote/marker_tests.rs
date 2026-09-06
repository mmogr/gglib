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
