//! This machine's door: where a client here reaches the endpoint, and the
//! key it carries, which must be the key the endpoint's bearer policy admits
//! (`gglib_core::access::BearerPolicy::current` is its twin).

use std::time::{Duration, Instant};

use gglib_core::SettingsUpdate;
use gglib_core::services::SETTINGS_CACHE_TTL;
use gglib_runtime::proxy::ProxyConfig;

use super::*;

/// Store `key` as the endpoint's `proxy_api_key`.
async fn store_key(proxy: &ProxyOps, key: &str) {
    proxy
        .core
        .settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(Some(key.to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the key is stored");
}

/// Start the endpoint on `host`, on a port the OS picks.
async fn start(proxy: &ProxyOps, host: &str) -> SocketAddr {
    let config = ProxyConfig {
        host: host.to_owned(),
        port: 0,
        ..ProxyConfig::default()
    };
    proxy
        .start(config, None)
        .await
        .expect("the endpoint starts")
}

/// The status a request to the endpoint behind `door` gets with `key`.
async fn status_with(door: &Door, key: &str) -> u16 {
    gglib_proxy::loopback::client()
        .get(format!("{}/v1/models", door.root_url))
        .bearer_auth(key)
        .send()
        .await
        .expect("the endpoint answers")
        .status()
        .as_u16()
}

#[tokio::test]
async fn an_endpoint_that_is_off_has_no_door_and_is_not_started() {
    let (_, proxy) = crate::test_support::test_core_and_proxy().await;
    let closed = proxy.door().await.expect_err("the endpoint is off");
    assert!(matches!(closed, DoorClosed::Off), "{closed:?}");
    assert_eq!(
        GuiError::from(closed).to_string(),
        "validation failed: the endpoint is off: turn it on first (`gglib proxy`, or Turn on)"
    );
    assert!(matches!(proxy.status().await, ProxyStatus::Stopped));
}

/// A wildcard bind names no reachable address, so the door is on loopback,
/// with no `/v1`; the key is the one the endpoint minted for that bind.
#[tokio::test]
async fn a_wildcard_bind_is_dialled_on_loopback_with_the_key_it_minted() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let addr = start(&proxy, "0.0.0.0").await;
    let door = proxy.door().await.expect("a door");
    let minted = core.settings().get().await.unwrap().proxy_api_key;
    proxy.stop().await.expect("stop");

    assert_eq!(door.root_url, format!("http://127.0.0.1:{}", addr.port()));
    assert!(minted.is_some(), "a wildcard bind mints a key");
    assert_eq!(door.key, minted);
    assert_eq!(door.name, None, "this machine's door names no machine");
}

/// A concrete bind is dialled where it is. A test cannot bind a LAN address
/// it does not have, so this is the door built from one.
#[test]
fn a_concrete_bind_is_dialled_where_it_is() {
    let door = local_door("192.168.1.5:9000".parse().unwrap(), None);
    assert_eq!(door.root_url, "http://192.168.1.5:9000");
}

/// An endpoint started open reports no key, and then picks up a key stored
/// later through its tracking policy. The door must carry it.
#[tokio::test]
async fn a_key_stored_after_an_open_start_is_carried() {
    let (_, proxy) = crate::test_support::test_core_and_proxy().await;
    start(&proxy, "127.0.0.1").await;
    let before = proxy.door().await.expect("a door").key;
    store_key(&proxy, "stored-later").await;
    let after = proxy.door().await.expect("a door").key;
    proxy.stop().await.expect("stop");

    assert_eq!(before, None);
    assert_eq!(after.as_deref(), Some("stored-later"));
}

/// The key alignment. An endpoint started with a stored key tracks the
/// setting, so after a rotation it admits the rotated key and refuses the
/// one it started with, once its settings cache has caught up. The door
/// carries the rotated key, not the start key.
#[tokio::test]
async fn after_a_rotation_the_doors_key_is_the_one_the_endpoint_admits() {
    let (_, proxy) = crate::test_support::test_core_and_proxy().await;
    store_key(&proxy, "k-start").await;
    start(&proxy, "127.0.0.1").await;
    let door = proxy.door().await.expect("a door");
    assert_eq!(door.key.as_deref(), Some("k-start"));
    assert_ne!(status_with(&door, "k-start").await, 401);

    store_key(&proxy, "k-rotated").await;
    let deadline = Instant::now() + SETTINGS_CACHE_TTL + Duration::from_secs(5);
    while status_with(&door, "k-start").await != 401 {
        assert!(
            Instant::now() < deadline,
            "the endpoint never stopped admitting the start key"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let rotated = proxy.door().await.expect("a door");
    let admitted = status_with(&rotated, rotated.key.as_deref().unwrap_or_default()).await;
    proxy.stop().await.expect("stop");

    assert_eq!(rotated.key.as_deref(), Some("k-rotated"));
    assert_ne!(admitted, 401, "the door's key is refused");
}
