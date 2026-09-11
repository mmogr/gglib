//! Per-device keys, over a real pipe: what `forget` cuts, and what the device
//! gate refuses.
//!
//! These are the two properties the redesign turns on, and neither can be
//! shown by setting headers by hand. `forget` is `remove_token` at the *edge*,
//! so only a live listener can be asked whether it still admits. And the
//! device gate exists because of what modelpipe does with
//! `ServeOptions::backend_auth` on a request a one-time **grant** admitted —
//! which means the grant has to be real, and so does the rewrite.
//!
//! The fixture they share is `fixtures::tunnel`, whose header records why
//! this needs no network.

use std::num::NonZeroU8;
use std::time::Duration;

use reqwest::StatusCode;

mod fixtures;
use fixtures::tunnel::{
    CODE, DEVICE, DEVICE_KEY, get, spawn_proxy, spawn_proxy_demanding, tunnel_to,
};

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

/// The window the device gate closes, with the grant real rather than
/// described.
///
/// A pairing grant admits **one** request at any path the holder likes — the
/// edge cannot scope it — and `backend_auth` rewrites that request's
/// `Authorization` to the backend's own credential like any other. So without
/// the gate, one correctly guessed six-digit code would buy a single fully
/// authenticated request to any protected route, `POST /v1/proxy/shutdown`
/// among them, which is irreversible without physical access.
///
/// What distinguishes it is the absence of `X-Modelpipe-Device`: the edge
/// writes that only when a *named* token admitted.
#[tokio::test]
async fn a_grant_admitted_request_reaches_no_protected_route() {
    let (proxy_url, cancel, _) = spawn_proxy().await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    // Warm the pipe on the paired device, so a refusal below is the gate and
    // not a connection that had not formed yet.
    assert_eq!(
        get(&format!("{base}/models"), Some(DEVICE_KEY))
            .await
            .status,
        StatusCode::OK
    );

    let code = "483920";
    serving
        .grant_once_bounded(
            code.to_owned(),
            Duration::from_secs(120),
            NonZeroU8::new(3).expect("3 is not zero"),
        )
        .expect("the pairing code is granted at the edge");

    let spent = get(&format!("{base}/models"), Some(code)).await;
    assert_eq!(
        spent.status,
        StatusCode::FORBIDDEN,
        "a grant let this in at the edge; the device gate is what stops it: {}",
        spent.body
    );
    assert_eq!(
        spent.json()["error"]["code"],
        "device_not_paired",
        "and it says which check refused it: {}",
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

/// A device that already holds a key gets nothing from `POST /v1/remote/pair`,
/// and cannot spend the invite meant for somebody else.
///
/// That route is outside the bearer group — it cannot demand the credential
/// it exists to hand out — so under `TokenPolicy::Named` a paired device
/// reaches it the way it reaches any other path: the edge admits it on its
/// own key and marks the request with its device name. Two things that buys a
/// device which has been compromised, both closed by refusing it before the
/// code is looked at — burning every invite a person types, three wrong codes
/// at a time, and three guesses per invite at a second identity that would
/// outlive the first being forgotten.
///
/// The markers are set by hand here rather than piped: what is under test is
/// the handler's reading of them, and a real tunnel cannot produce the
/// grant-admitted and device-admitted requests in one session anyway. The
/// peer fingerprint is deliberately the same on both — it names a process on
/// the far side, not a device, and must never become the discriminator.
#[tokio::test]
async fn a_device_that_is_already_paired_cannot_redeem_or_burn_a_code() {
    let (proxy_url, cancel, gateway) = spawn_proxy().await;

    for attempt in 0..5 {
        let res = reqwest::Client::new()
            .post(format!("{proxy_url}/v1/remote/pair"))
            .header("via", "1.1 modelpipe")
            .header("x-modelpipe-peer", "3ca82708b995")
            .header("x-modelpipe-device", DEVICE)
            .json(&serde_json::json!({ "code": CODE }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "attempt {attempt}: the right code is still refused on a device key"
        );
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(
            body["error"]["code"], "invalid_pairing_code",
            "flat refusal"
        );
    }

    // Five is past the three-attempt burn, and the invite is untouched: the
    // handler never reached the gateway, so a paired device cannot spend
    // somebody else's budget.
    let honest = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/remote/pair"))
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", "3ca82708b995")
        .json(&serde_json::json!({ "code": CODE, "name": "Matt\u{2019}s laptop" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        honest.status(),
        StatusCode::OK,
        "the invite a person is holding survived a paired device hammering it"
    );
    assert_eq!(
        gateway.paired_name.lock().unwrap().as_deref(),
        Some("Matt\u{2019}s laptop"),
        "and the grant-admitted request is the one that redeemed it"
    );

    cancel.cancel();
}
