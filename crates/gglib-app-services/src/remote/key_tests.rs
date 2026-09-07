//! Tests for [`super`] — which key the tunnel enforces, and when the mint
//! for it is written down.
//!
//! A sibling file rather than an inline `mod tests`, for the reason
//! `settings_remote_tests.rs` is one: `key.rs` is close enough to the 300-line
//! budget that the tests would push it over.

use tokio_util::sync::CancellationToken;

use super::*;

#[test]
fn a_pinned_flag_key_wins_and_is_marked_pinned() {
    let decision = decide(
        Some(("flag-key".to_owned(), ApiKeySource::Flag)),
        Some("stored-key"),
    );
    assert_eq!(
        decision,
        KeyDecision::Use {
            key: "flag-key".to_owned(),
            pinned: true
        }
    );
}

#[test]
fn a_key_the_proxy_resolved_from_settings_is_used_and_tracks() {
    for source in [ApiKeySource::Settings, ApiKeySource::Generated] {
        let decision = decide(Some(("running".to_owned(), source)), Some("stored"));
        assert_eq!(
            decision,
            KeyDecision::Use {
                key: "running".to_owned(),
                pinned: false
            }
        );
    }
}

#[test]
fn a_stored_key_is_used_when_the_proxy_has_none_yet() {
    let decision = decide(None, Some("  stored-key  "));
    assert_eq!(
        decision,
        KeyDecision::Use {
            key: "stored-key".to_owned(),
            pinned: false
        }
    );
}

#[test]
fn nothing_anywhere_mints_a_fresh_key() {
    for stored in [None, Some(""), Some("   ")] {
        match decide(None, stored) {
            KeyDecision::Mint(key) => assert!(!key.is_empty()),
            other => panic!("expected Mint, got {other:?}"),
        }
    }
}

#[test]
fn two_mints_differ() {
    let (KeyDecision::Mint(a), KeyDecision::Mint(b)) = (decide(None, None), decide(None, None))
    else {
        panic!("both must mint");
    };
    assert_ne!(a, b);
}

/// The whole reason `settle` and `commit` are two calls. Between them
/// sits `modelpipe::serve`, which fails on a bad relay value, an
/// unreachable backend or a socket the machine will not give up — and
/// none of those failures may leave the local proxy demanding a key.
#[tokio::test]
async fn settling_a_minted_key_writes_nothing_yet() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;

    let settled = settle(&proxy, &core)
        .await
        .expect("settling reads settings");

    assert!(
        !settled.key.is_empty(),
        "nothing was enforced, so one is minted"
    );
    assert!(settled.minted);
    assert!(
        core.settings()
            .get()
            .await
            .expect("settings")
            .proxy_api_key
            .is_none(),
        "the mint is not in settings until the tunnel it is for exists"
    );
}

/// And then it does write it. The write comes first and the settings-cache
/// wait after, which is what lets this read the key out from under a commit
/// still in its wait rather than sitting out the window: a test that had to
/// wait would be five seconds long, and one that skipped the write would
/// prove nothing.
#[tokio::test]
async fn committing_a_minted_key_stores_it_before_it_waits_for_the_proxy() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let settled = settle(&proxy, &core)
        .await
        .expect("settling reads settings");
    let minted = settled.key.clone();

    let committing = tokio::spawn({
        let core = std::sync::Arc::clone(&core);
        async move { settled.commit(&core, &CancellationToken::new()).await }
    });

    let stored = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(key) = core.settings().get().await.expect("settings").proxy_api_key {
                return key;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a minted key is stored before the wait, not after it");

    assert_eq!(stored, minted);
    committing.abort();
}

/// The wait is for a key the proxy has not seen. A key that was already
/// stored is already being enforced, so committing it must neither
/// rewrite it nor spend a settings-cache window doing nothing — which is
/// what the timeout here is asserting.
#[tokio::test]
async fn committing_a_key_that_was_already_stored_writes_nothing_and_waits_for_nothing() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    core.settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(Some("already-enforced".to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("settings update");

    let settled = settle(&proxy, &core)
        .await
        .expect("settling reads settings");
    assert_eq!(settled.key, "already-enforced");

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        settled.commit(&core, &CancellationToken::new()),
    )
    .await
    .expect("a key nothing minted must not wait out the settings cache")
    .expect("committing nothing cannot fail");

    assert_eq!(
        core.settings()
            .get()
            .await
            .expect("settings")
            .proxy_api_key
            .as_deref(),
        Some("already-enforced"),
        "and the stored key is left exactly as it was"
    );
}

/// A `disable` that arrives while a mint is waiting out the settings cache
/// is not made to wait for it. The key is already written by this point and
/// the caller is giving up, so the window buys nothing — and holding it would
/// give the serve side back exactly the stall that reserving the slot rather
/// than locking it exists to remove.
#[tokio::test]
async fn a_cancelled_commit_stops_waiting_for_the_settings_cache() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let settled = settle(&proxy, &core)
        .await
        .expect("settling reads settings");
    assert!(
        settled.minted,
        "an unset key is minted, and a mint is what waits"
    );

    let cancel = CancellationToken::new();
    cancel.cancel();

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        settled.commit(&core, &cancel),
    )
    .await
    .expect("a cancelled commit must not sit out the settings-cache window")
    .expect("committing a minted key cannot fail");
}
