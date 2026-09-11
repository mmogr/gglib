//! The seam nothing else tests: a real gglib proxy, behind a real modelpipe
//! tunnel, reached through a real connect listener.
//!
//! Every other remote test in this crate *synthesises* the tunnel. It sets
//! `via`, `x-modelpipe-peer` and `x-modelpipe-device` by hand against a
//! `StubGateway`, which proves what the proxy does with a request that claims
//! to have come through a tunnel — not what happens to one that actually did.
//! modelpipe's own suite has the opposite gap: it pairs two live endpoints,
//! but against a `MockBackend`, so it proves the pipe and not what is behind
//! it.
//!
//! The two halves had never met. No test in either repository stood up a
//! gglib proxy and reached it through a pipe, which meant the least-tested
//! thing in the whole feature was the feature.
//!
//! This file is about a request crossing the pipe at all;
//! `integration_remote_devices.rs` is about *which* credentials get to cross
//! it. Both run over `fixtures::tunnel`, whose header records why this needs
//! no network.

use reqwest::StatusCode;

mod fixtures;
use fixtures::tunnel::{DEVICE_KEY, PROXY_KEY, get, spawn_proxy, tunnel_to};

/// The claim the whole feature rests on: a request sent to a loopback port on
/// *this* side comes out of a gglib proxy on the far side, and comes back.
#[tokio::test]
async fn a_request_through_the_tunnel_reaches_the_proxy_and_is_answered() {
    let (proxy_url, cancel, gateway) = spawn_proxy().await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    let response = get(&format!("{base}/models"), Some(DEVICE_KEY)).await;
    assert_eq!(
        response.status,
        StatusCode::OK,
        "a request on a paired device's key reaches the proxy: {}",
        response.body
    );

    // The catalog is empty, so the interesting part is the shape rather than
    // the contents: this is gglib's own `/v1/models` answering, not the
    // tunnel inventing a reply.
    let body = response.json();
    assert_eq!(body["object"], "list", "gglib's own models payload: {body}");

    // And the whole of `backend_auth` in one assertion: the device presented
    // its own key, the proxy demanded a different one, and the request was
    // answered — so the edge swapped the credential in flight and the
    // device's key never reached gglib.
    assert_eq!(
        gateway.last_device.lock().unwrap().as_deref(),
        Some(fixtures::tunnel::DEVICE),
        "the proxy was told which device this was"
    );

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}

/// The other half of decision 2: the tunnel edge refuses a bad bearer before
/// a byte reaches gglib. The naive embedding this ADR exists to prevent would
/// answer this request, because the proxy sees a loopback bind and a loopback
/// `Host` and trusts both.
#[tokio::test]
async fn a_request_through_the_tunnel_without_the_key_is_refused() {
    let (proxy_url, cancel, _) = spawn_proxy().await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    // Warm the pipe with a good request first, so a refusal below cannot be
    // a connection that had not formed yet wearing a 401's clothes.
    assert_eq!(
        get(&format!("{base}/models"), Some(DEVICE_KEY))
            .await
            .status,
        StatusCode::OK
    );

    let refused = get(&format!("{base}/models"), None).await;
    assert_eq!(
        refused.status,
        StatusCode::UNAUTHORIZED,
        "reaching loopback through a tunnel is not being on this machine"
    );

    let wrong = get(&format!("{base}/models"), Some("sk-not-the-key")).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);

    // The backend's own credential is not a tunnel credential. Under
    // `TokenPolicy::Named` the primary is absent, so the key that opens the
    // local proxy opens nothing from the far side — which is what makes
    // `forget` mean anything: there is no shared key left to fall back on.
    let backend_key = get(&format!("{base}/models"), Some(PROXY_KEY)).await;
    assert_eq!(
        backend_key.status,
        StatusCode::UNAUTHORIZED,
        "the proxy's own key must not admit at the edge"
    );

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}
