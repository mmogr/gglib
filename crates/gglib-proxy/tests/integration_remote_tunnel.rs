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
//! This file is about a request crossing the pipe at all, about the markers
//! it carries out of the far end, and about a device pairing through the
//! edge; `integration_remote_devices.rs` is about *which* credentials get to
//! cross it. Both run over `fixtures::tunnel`, whose header records why this
//! needs no network.

use std::time::Duration;

use reqwest::{Client, StatusCode};

mod fixtures;
use fixtures::recorder::spawn_recorder;
use fixtures::tunnel::{Answer, DEVICE, DEVICE_KEY, PROXY_KEY, get, spawn_proxy, tunnel_to};

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

/// A peer fingerprint no endpoint has, for a client to claim.
const FORGED_PEER: &str = "000000000000";

/// Send `bearer` with all three tunnel markers written by the client: `Via`
/// in the edge's own words, [`FORGED_PEER`], and `device` as the device name.
///
/// Sent once rather than through `get`'s retry loop, so a caller warms the
/// pipe first.
async fn forging(url: &str, bearer: &str, device: &str) -> Answer {
    let response = Client::new()
        .get(url)
        .bearer_auth(bearer)
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", FORGED_PEER)
        .header("x-modelpipe-device", device)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("a warm pipe carries the request");
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Answer { status, body }
}

/// The fingerprint the edge knows the connect side by.
fn edge_peer(serving: &modelpipe::ServeHandle) -> String {
    serving
        .peers()
        .first()
        .map(|view| view.fingerprint.clone())
        .expect("the connect side is a peer of the listener")
}

/// A paired device that writes the markers itself reaches the proxy carrying
/// one of each, and each is the edge's: `Via: 1.1 modelpipe`, the fingerprint
/// of the endpoint the device really connected from, and the name its key is
/// held under rather than the one it claimed.
///
/// `remote::marker` reads whatever arrives, on the promise that the edge
/// always overwrites. This is that promise, kept over a real pipe.
#[tokio::test]
async fn a_client_supplied_device_header_is_replaced_by_the_edges_own() {
    let (proxy_url, cancel, gateway) = spawn_proxy().await;
    let (hop_url, seen) = spawn_recorder(proxy_url).await;
    let (serving, connected, base) = tunnel_to(&hop_url).await;

    // Warm the pipe, so the request below is carried rather than retried.
    assert_eq!(
        get(&format!("{base}/models"), Some(DEVICE_KEY))
            .await
            .status,
        StatusCode::OK
    );

    // Well formed, so the proxy would read it if it arrived, and not the name
    // this key is held under, so reading it would show.
    let answer = forging(&format!("{base}/models"), DEVICE_KEY, "dev-ffffffff").await;
    assert_eq!(
        answer.status,
        StatusCode::OK,
        "a paired device that forges the markers is still served: {}",
        answer.body
    );

    let peer = edge_peer(&serving);
    let arrived = seen
        .lock()
        .unwrap()
        .last()
        .cloned()
        .expect("the hop saw the request");
    for (name, edges) in [
        ("via", "1.1 modelpipe"),
        ("x-modelpipe-peer", peer.as_str()),
        ("x-modelpipe-device", DEVICE),
    ] {
        let values: Vec<&str> = arrived
            .get_all(name)
            .iter()
            .map(|value| value.to_str().unwrap_or("<not text>"))
            .collect();
        assert_eq!(
            values,
            [edges],
            "exactly one `{name}`, and it is the edge's: {arrived:?}"
        );
    }

    // And what the proxy made of it: the device and the peer the edge named.
    assert_eq!(gateway.last_device.lock().unwrap().as_deref(), Some(DEVICE));
    assert_eq!(
        gateway.last_peer.lock().unwrap().as_deref(),
        Some(peer.as_str())
    );

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}

/// A device pairs through the real edge route, and the key it is handed
/// admits it to the proxy under the name it was invited as.
///
/// This is `gglib remote invite` on one side and `gglib remote join` on the
/// other with the daemon and the CLI taken out: `invite`, then `arm`, then
/// `modelpipe::pair` over the pipe, then an ordinary request with the key
/// that came back. The pairing request itself never reaches the proxy, which
/// is why nothing on this side is asked about it.
#[tokio::test]
async fn a_device_pairs_through_the_edge_and_its_key_admits_it() {
    let (proxy_url, cancel, gateway) = spawn_proxy().await;
    let (serving, connected, _base) = tunnel_to(&proxy_url).await;

    let invited = serving
        .invite(modelpipe::InviteOptions::default())
        .expect("the listener invites a device");
    invited.arm();

    let mut opts = modelpipe::ConnectOptions::default();
    opts.discovery = false;
    opts.port_mapping = false;
    let paired = modelpipe::pair(
        invited.pairing(),
        Some("Laptop"),
        opts,
        Duration::from_secs(20),
    )
    .await
    .expect("the device pairs over the pipe");
    assert_eq!(
        paired.device,
        invited.device(),
        "under the name it was invited as"
    );
    assert_eq!(
        paired.api_key,
        invited.api_key(),
        "holding the key minted for it"
    );

    let answer = get(
        &format!("{}/models", paired.handle.base_url()),
        Some(&paired.api_key),
    )
    .await;
    assert_eq!(
        answer.status,
        StatusCode::OK,
        "the key it was handed admits it: {}",
        answer.body
    );
    assert_eq!(
        gateway.last_device.lock().unwrap().as_deref(),
        Some(invited.device()),
        "and the proxy is told which device it is"
    );

    paired.handle.shutdown().await;
    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}
