//! What a paired machine can reach, pinned.
//!
//! The remote tunnel (ADR 0012) forwards to exactly one backend — this
//! proxy — so the proxy's route table *is* the list of what `--remote`
//! reaches, and ADR 0013 draws a line through it: a route may let a paired
//! machine **use** what is on this one (a turn, its catalogue, its status,
//! stopping it) and may never let it **change** what is on this one (pull
//! or remove a model, write a setting). Every route below is on the use
//! side. A route added to the proxy lands on the far side of the tunnel
//! whether or not anyone meant it to, so this test fails until the new
//! route is written into the list — which is the moment to ask which side
//! of the line it is on.
//!
//! Read from the source rather than the router, because axum's `Router`
//! does not enumerate its routes and a second, data-driven table would be
//! the thing that drifts.

use crate::tunnel_routes::TUNNEL_REACHABLE;

/// The paths `router.rs` registers, in source order: the first quoted
/// string after each `.route(`, wherever rustfmt put the line break.
fn registered() -> Vec<String> {
    include_str!("router.rs")
        .split(".route(")
        .skip(1)
        .filter_map(|rest| rest.split('"').nth(1))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_route_the_proxy_serves_is_one_a_paired_machine_may_use() {
    let mut found = registered();
    let mut pinned: Vec<String> = TUNNEL_REACHABLE.iter().map(|s| (*s).to_owned()).collect();
    found.sort();
    pinned.sort();
    assert_eq!(
        found, pinned,
        "the proxy's routes are what the tunnel carries; a route added here \
         reaches every paired machine — decide which side of ADR 0013's line \
         it is on, then add it to TUNNEL_REACHABLE"
    );
}

/// The parser above is the guard, so the guard has to be shown to see
/// routes at all: a `router.rs` rewritten in a shape it does not read would
/// otherwise pass by finding nothing on both sides.
#[test]
fn the_route_reader_finds_the_routes() {
    assert!(
        registered().len() >= 8,
        "the reader found almost nothing; router.rs changed shape"
    );
}
