//! Tests for [`super::bearer_matches`] — what counts as presenting the key.

use super::bearer_matches;

const KEY: &str = "sk-secret123";

/// The case the change exists for. Every spelling of the scheme is the same
/// scheme, and a client that picks one is not sending a wrong credential.
#[test]
fn every_spelling_of_the_scheme_is_accepted() {
    for header in [
        "Bearer sk-secret123",
        "bearer sk-secret123",
        "BEARER sk-secret123",
        "BeArEr sk-secret123",
    ] {
        assert!(
            bearer_matches(Some(header), KEY),
            "{header:?} presents the key"
        );
    }
}

/// `1*SP` between the scheme and the credential, and trailing whitespace is
/// not part of the credential.
#[test]
fn the_separator_may_be_more_than_one_space() {
    assert!(bearer_matches(Some("Bearer  sk-secret123"), KEY));
    assert!(bearer_matches(Some("Bearer sk-secret123 "), KEY));
}

/// The grammar says `1*SP`, so a tab is not the separator. Rejected rather
/// than normalised: nothing sends this, and quietly accepting it would widen
/// the header shapes this endpoint claims to understand for no caller's sake.
#[test]
fn a_tab_is_not_the_separator() {
    assert!(!bearer_matches(Some("Bearer\tsk-secret123"), KEY));
}

/// Every flavour of a wrong credential a lenient comparison would let past.
/// These all failed before the change and must keep failing after it — the
/// point was to stop rejecting *correct* requests, not to start accepting
/// incorrect ones.
#[test]
fn every_flavour_of_wrong_credential_is_still_refused() {
    let cases: Vec<(&str, Option<&str>)> = vec![
        ("no Authorization header at all", None),
        ("a different key", Some("Bearer nope")),
        (
            "the right key under a different scheme",
            Some("Basic sk-secret123"),
        ),
        ("the key with no scheme", Some("sk-secret123")),
        ("a prefix of the key", Some("Bearer sk-secret")),
        ("the key plus a suffix", Some("Bearer sk-secret1234")),
        ("an empty header", Some("")),
        ("the scheme with no credential", Some("Bearer")),
        ("the scheme and a space, nothing more", Some("Bearer ")),
        ("the scheme and only whitespace", Some("Bearer    ")),
    ];

    for (why, header) in cases {
        assert!(
            !bearer_matches(header, KEY),
            "{why} must not authenticate: {header:?}"
        );
    }
}

/// A blank expectation cannot be satisfied by anything, including a blank
/// credential. Settings validation refuses to store one; this is the second
/// line, so a blank arriving by some other route fails closed rather than
/// admitting every caller.
#[test]
fn a_blank_expected_key_admits_nobody() {
    for header in [None, Some("Bearer "), Some("Bearer x"), Some("")] {
        assert!(
            !bearer_matches(header, ""),
            "a blank key must admit nobody: {header:?}"
        );
    }
}

/// The scheme is compared case-insensitively; the credential is not.
#[test]
fn the_credential_stays_case_sensitive() {
    assert!(!bearer_matches(Some("Bearer SK-SECRET123"), KEY));
    assert!(!bearer_matches(Some("bearer Sk-Secret123"), KEY));
}

mod policy {
    use std::sync::{Arc, Mutex};

    use super::super::BearerPolicy;
    use crate::ports::{RepositoryError, SettingsRepository};
    use crate::services::SettingsCache;
    use crate::settings::Settings;

    /// A repository whose stored key can be changed underneath a live policy,
    /// standing in for the CLI writing the same database from another process.
    struct Rotatable(Mutex<Settings>);

    impl Rotatable {
        fn new(key: Option<&str>) -> Arc<Self> {
            let mut settings = Settings::with_defaults();
            settings.proxy_api_key = key.map(str::to_owned);
            Arc::new(Self(Mutex::new(settings)))
        }

        fn rotate_to(&self, key: Option<&str>) {
            self.0.lock().unwrap().proxy_api_key = key.map(str::to_owned);
        }
    }

    #[async_trait::async_trait]
    impl SettingsRepository for Rotatable {
        async fn load(&self) -> Result<Settings, RepositoryError> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
            *self.0.lock().unwrap() = settings.clone();
            Ok(())
        }
    }

    /// Zero TTL so a write is observed on the next read; the window itself is
    /// already covered by the cache's own tests.
    fn live(repo: Arc<Rotatable>) -> Arc<SettingsCache> {
        Arc::new(SettingsCache::with_ttl(repo, std::time::Duration::ZERO))
    }

    /// The blocker this exists for: a key rotated after the endpoint bound is
    /// the key it demands.
    #[tokio::test]
    async fn a_rotation_takes_effect_without_a_restart() {
        let repo = Rotatable::new(Some("old-key"));
        let policy = BearerPolicy::tracking(Some("old-key"), live(Arc::clone(&repo)));

        assert!(policy.admits(Some("Bearer old-key")).await);

        repo.rotate_to(Some("new-key"));
        assert!(
            policy.admits(Some("Bearer new-key")).await,
            "the rotated key must be accepted"
        );
        assert!(
            !policy.admits(Some("Bearer old-key")).await,
            "the retired key must stop working"
        );
    }

    /// The other half of unfreezing: an endpoint that bound with no token can
    /// be closed without restarting it.
    #[tokio::test]
    async fn authentication_can_be_switched_on_at_runtime() {
        let repo = Rotatable::new(None);
        let policy = BearerPolicy::tracking(None, live(Arc::clone(&repo)));

        assert!(policy.admits(None).await, "no token configured is open");

        repo.rotate_to(Some("now-required"));
        assert!(!policy.admits(None).await, "setting a key closes the door");
        assert!(policy.admits(Some("Bearer now-required")).await);
    }

    /// And never off. A listener bound off loopback had a token minted for it
    /// precisely because it is reachable; clearing the setting must not
    /// silently expose it.
    #[tokio::test]
    async fn clearing_the_setting_does_not_reopen_a_closed_endpoint() {
        let repo = Rotatable::new(Some("bound-with"));
        let policy = BearerPolicy::tracking(Some("bound-with"), live(Arc::clone(&repo)));

        repo.rotate_to(None);
        assert!(
            !policy.admits(None).await,
            "an endpoint that required a token must keep requiring one"
        );
        assert!(
            policy.admits(Some("Bearer bound-with")).await,
            "it falls back to the token it bound with"
        );
    }

    /// A blank stored value is not a credential. Settings validation refuses
    /// one, and if it arrives anyway it must not read as "auth is off".
    #[tokio::test]
    async fn a_blank_stored_key_falls_back_rather_than_opening() {
        let repo = Rotatable::new(Some("bound-with"));
        let policy = BearerPolicy::tracking(Some("bound-with"), live(Arc::clone(&repo)));

        repo.rotate_to(Some("   "));
        assert!(!policy.admits(None).await);
        assert!(policy.admits(Some("Bearer bound-with")).await);
    }

    /// `--api-key` and `GGLIB_API_KEY` outrank the stored setting, so a
    /// settings write must not replace one — that would both invert the
    /// documented precedence and lock out the operator who passed it.
    #[tokio::test]
    async fn a_pinned_key_ignores_the_stored_setting() {
        let policy = BearerPolicy::pinned("from-the-flag");

        assert!(policy.admits(Some("Bearer from-the-flag")).await);
        assert!(!policy.admits(Some("Bearer whatever-is-stored")).await);
    }
}
