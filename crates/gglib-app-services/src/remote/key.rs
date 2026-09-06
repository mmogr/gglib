//! Which key the tunnel enforces, and whether one has to be minted first.
//!
//! Pure: the decision over what the proxy currently demands and what
//! settings hold. `RemoteOps` does the persisting and the waiting.

use gglib_core::ApiKeySource;
use gglib_core::access::generate_api_key;

/// The key the tunnel will enforce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum KeyDecision {
    /// Enforce this key; the proxy already demands it. Nothing to write.
    Use {
        /// The key.
        key: String,
        /// Whether it came from a flag or environment variable, in which
        /// case a settings rotation will never change it and the poller has
        /// nothing to watch.
        pinned: bool,
    },
    /// Nothing is enforced anywhere yet. Persist this freshly minted key to
    /// `proxy_api_key`, wait for the proxy's tracking policy to pick it up,
    /// then enforce it.
    Mint(String),
}

/// Decide the key, in the order ADR 0012 gives.
///
/// 1. What the running proxy demands, when it demands something. A key from
///    `--api-key`/`GGLIB_API_KEY` is pinned: it never appears in settings,
///    and handing the tunnel the stored value would give it a credential
///    the proxy refuses.
/// 2. The stored `proxy_api_key`, when the proxy has not (yet) resolved one
///    — it will, within a cache window, because the tracking policy reads
///    the same setting.
/// 3. A fresh key, to be persisted. This is the loopback default, where
///    nothing minted a key because nothing was reachable. Enabling the
///    tunnel is exactly the moment that stops being true.
pub(super) fn decide(
    effective: Option<(String, ApiKeySource)>,
    stored: Option<&str>,
) -> KeyDecision {
    if let Some((key, source)) = effective {
        return KeyDecision::Use {
            key,
            pinned: source == ApiKeySource::Flag,
        };
    }
    if let Some(stored) = stored.map(str::trim).filter(|s| !s.is_empty()) {
        return KeyDecision::Use {
            key: stored.to_owned(),
            pinned: false,
        };
    }
    KeyDecision::Mint(generate_api_key())
}

#[cfg(test)]
mod tests {
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
}
