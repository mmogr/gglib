//! Per-device keys, over a real pipe: what `forget` cuts, and what the device
//! gate refuses.
//!
//! These are the two properties the redesign turns on, and neither can be
//! shown by setting headers by hand. `forget` is `remove_token` at the *edge*,
//! so only a live listener can be asked whether it still admits. And what a
//! pairing code buys has to be asked of a real edge, which answers the code
//! itself and refuses it as a key anywhere else.
//!
//! The fixture they share is `fixtures::tunnel`, whose header records why
//! this needs no network.

use reqwest::StatusCode;

mod fixtures;
use fixtures::tunnel::{DEVICE, DEVICE_KEY, get, spawn_proxy, spawn_proxy_demanding, tunnel_to};

/// Forgetting a device, end to end: the key that worked a moment ago stops
/// being admitted, and nothing else is disturbed.
///
/// This is the whole point of the change. Under one shared key the only
/// revocation was a rotation, which un-paired every device at once; under
/// named tokens a lost laptop costs the laptop.
#[tokio::test]
async fn a_forgotten_device_stops_being_admitted() {
    let (proxy_url, cancel, _) = spawn_proxy().await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    assert_eq!(
        get(&format!("{base}/models"), Some(DEVICE_KEY))
            .await
            .status,
        StatusCode::OK,
        "the device is paired to begin with, or the refusal below is vacuous"
    );

    assert!(
        serving.remove_token(DEVICE),
        "the listener was holding the device this forgets"
    );

    let refused = get(&format!("{base}/models"), Some(DEVICE_KEY)).await;
    assert_eq!(
        refused.status,
        StatusCode::UNAUTHORIZED,
        "a forgotten device's key must stop admitting: {}",
        refused.body
    );

    // And the machine is still serving — `forget` retires one credential, it
    // does not take the tunnel down. A second device would still be working,
    // which is the difference from a rotation.
    serving
        .add_token("dev-11112222", "sk-zzq-the-phones-key".to_owned())
        .expect("another device joins the same listener");
    assert_eq!(
        get(&format!("{base}/models"), Some("sk-zzq-the-phones-key"))
            .await
            .status,
        StatusCode::OK,
        "forgetting one device left the listener serving the others"
    );

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}

/// A live pairing code buys nothing at the proxy, with the invite real rather
/// than described.
///
/// While gglib paired through a one-time grant, a live code admitted **one**
/// request at any path its holder liked, and the device gate was what stopped
/// it reaching a protected route. Under modelpipe 0.6 a code is presented only
/// to the edge's own pairing route, which never reaches the backend; used as a
/// bearer anywhere else it is a key the edge does not hold, refused before the
/// proxy sees the request.
#[tokio::test]
async fn a_live_pairing_code_used_as_a_key_is_refused_at_the_edge() {
    let (proxy_url, cancel, _) = spawn_proxy().await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    // Warm the pipe on the paired device, so a refusal below is the edge's and
    // not a connection that had not formed yet.
    assert_eq!(
        get(&format!("{base}/models"), Some(DEVICE_KEY))
            .await
            .status,
        StatusCode::OK
    );

    let invited = serving
        .invite(modelpipe::InviteOptions::default())
        .expect("the listener invites a device");
    invited.arm();

    let spent = get(&format!("{base}/models"), Some(invited.code().as_str())).await;
    assert_eq!(
        spent.status,
        StatusCode::UNAUTHORIZED,
        "a code is a key nowhere but the edge's pairing route: {}",
        spent.body
    );

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}

/// A rotation of the *proxy's* key does not un-pair anybody.
///
/// This is the paragraph the redesign deletes from `docs/remote.md`. Under one
/// shared key a rotation was a revocation of every device at once; the edge
/// now holds a key per device and presents the backend's own credential in
/// their place, so the only thing a rotation changes is that last hop.
///
/// Driven through `set_backend_auth`, which is the single call
/// `remote::rotation::rotation_poll` makes when it sees the key change. The
/// regression it guards is `set_token` coming back: that writes a *primary*
/// onto a `Named` listener, which would quietly restore a shared key that
/// admits everywhere and make `forget` incomplete from that moment on, with
/// no error and no log line. Hence the `token_names` assertion — it is not
/// decoration.
#[tokio::test]
async fn a_rotation_leaves_every_paired_device_where_it_was() {
    // A proxy that already demands the rotated key, with the tunnel armed on
    // the old one: the window between a rotation landing in settings and the
    // poller noticing it, held still.
    let rotated = "sk-zzq-the-rotated-backend-key";
    let (proxy_url, cancel, _) = spawn_proxy_demanding(rotated).await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    let stale = get(&format!("{base}/models"), Some(DEVICE_KEY)).await;
    assert_eq!(
        stale.status,
        StatusCode::UNAUTHORIZED,
        "the edge still presents the old credential upstream: {}",
        stale.body
    );

    serving
        .set_backend_auth(Some(rotated.to_owned()))
        .expect("the listener follows the rotation");

    let after = get(&format!("{base}/models"), Some(DEVICE_KEY)).await;
    assert_eq!(
        after.status,
        StatusCode::OK,
        "the same device, the same key, no re-pair: {}",
        after.body
    );
    assert_eq!(
        serving.token_names(),
        vec![DEVICE.to_owned()],
        "a rotation must not add, drop or rewrite a device token"
    );

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}
