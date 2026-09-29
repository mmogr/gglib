//! The scope a request is served in, from what the marker read.

use axum::http::HeaderMap;
use gglib_core::ports::RunScope;

use super::scope_of;
use crate::remote::Tunnelled;

fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(*name, value.parse().unwrap());
    }
    map
}

fn scope(pairs: &[(&'static str, &'static str)]) -> Option<RunScope> {
    scope_of(Tunnelled::from_headers(&headers(pairs)).as_ref())
}

#[test]
fn a_request_with_no_markers_is_this_machine() {
    assert_eq!(scope(&[]), Some(RunScope::Local));
}

/// The device header alone is not a tunnelled request, so a local client
/// that writes it is served exactly as one that does not.
#[test]
fn the_device_header_without_via_changes_nothing() {
    assert_eq!(
        scope(&[("x-modelpipe-device", "dev-0a1b2c3d")]),
        Some(RunScope::Local)
    );
}

#[test]
fn a_tunnelled_request_is_the_device_the_edge_named() {
    assert_eq!(
        scope(&[
            ("via", "1.1 modelpipe"),
            ("x-modelpipe-device", "dev-0a1b2c3d")
        ]),
        Some(RunScope::Device("dev-0a1b2c3d".to_owned()))
    );
}

#[test]
fn a_tunnelled_request_that_names_no_device_has_no_scope() {
    assert_eq!(scope(&[("via", "1.1 modelpipe")]), None);
}
