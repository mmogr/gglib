//! The door's two rules: the key the tunnel would enforce, never a minted
//! one, and a wildcard bind dialled on loopback.

use std::net::SocketAddr;

use gglib_core::{ApiKeySource, SettingsUpdate};

use super::door::{bearer, dial};
use crate::remote::key::enforced;

#[test]
fn the_running_proxys_key_wins_and_the_stored_key_is_the_fallback() {
    let running = Some(("running".to_owned(), ApiKeySource::Settings));
    assert_eq!(
        enforced(running, Some("stored")).map(|(k, _)| k).as_deref(),
        Some("running")
    );
    assert_eq!(
        enforced(None, Some("  stored  "))
            .map(|(k, _)| k)
            .as_deref(),
        Some("stored")
    );
    for nothing in [None, Some(""), Some("   ")] {
        assert_eq!(enforced(None, nothing), None, "nothing is minted");
    }
}

/// A proxy started with no key reports none, and then picks up a key
/// stored later through its tracking policy. The run must present it.
#[tokio::test]
async fn a_proxy_started_with_no_key_then_a_stored_key_is_sent_the_stored_key() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    assert_eq!(proxy.effective_api_key(), None);
    assert_eq!(bearer(&proxy, &core).await.unwrap(), None);

    core.settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(Some("stored-later".to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .unwrap();

    assert_eq!(
        bearer(&proxy, &core).await.unwrap().as_deref(),
        Some("stored-later")
    );
}

#[test]
fn only_a_wildcard_bind_is_rewritten_to_loopback() {
    let cases = [
        ("0.0.0.0:8080", "127.0.0.1:8080"),
        ("[::]:8080", "127.0.0.1:8080"),
        ("127.0.0.1:8080", "127.0.0.1:8080"),
        ("192.168.1.20:8080", "192.168.1.20:8080"),
    ];
    for (bound, dialled) in cases {
        let bound: SocketAddr = bound.parse().unwrap();
        assert_eq!(dial(bound), dialled.parse().unwrap(), "{bound}");
    }
}
