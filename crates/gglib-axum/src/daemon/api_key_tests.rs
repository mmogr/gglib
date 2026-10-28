//! Unit tests for the daemon's bearer token, [`super::resolve_daemon_api_key`].

use std::sync::{Arc, Mutex};

use gglib_core::Settings;
use gglib_core::ports::{CoreError, RepositoryError, SettingsChange, SettingsRepository};

use gglib_core::ApiKeySource;

use super::{api_key_banner, resolve_daemon_api_key};

/// A settings store that counts what is written to it.
struct Recording {
    stored: Mutex<Settings>,
    writes: Mutex<usize>,
}

impl Recording {
    fn with_key(key: Option<&str>) -> Arc<Self> {
        let mut settings = Settings::with_defaults();
        settings.proxy_api_key = key.map(str::to_owned);
        Arc::new(Self {
            stored: Mutex::new(settings),
            writes: Mutex::new(0),
        })
    }

    fn persisted_key(&self) -> Option<String> {
        self.stored.lock().unwrap().proxy_api_key.clone()
    }

    fn writes(&self) -> usize {
        *self.writes.lock().unwrap()
    }
}

#[async_trait::async_trait]
impl SettingsRepository for Recording {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(self.stored.lock().unwrap().clone())
    }
    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        *self.stored.lock().unwrap() = settings.clone();
        *self.writes.lock().unwrap() += 1;
        Ok(())
    }
    async fn modify(&self, change: &SettingsChange<'_>) -> Result<Settings, CoreError> {
        let mut stored = self.stored.lock().unwrap();
        change(&mut stored)?;
        *self.writes.lock().unwrap() += 1;
        Ok(stored.clone())
    }
}

fn repo(store: &Arc<Recording>) -> Arc<dyn SettingsRepository> {
    Arc::clone(store) as Arc<dyn SettingsRepository>
}

/// Off loopback a blank or absent key is minted and stored, once, and the
/// daemon that binds next reads that same key and writes nothing.
#[tokio::test]
async fn a_daemon_off_loopback_mints_a_key_over_a_blank_one_and_stores_it_once() {
    for stored in [None, Some(""), Some("   ")] {
        let store = Recording::with_key(stored);

        let minted = resolve_daemon_api_key("0.0.0.0", &repo(&store))
            .await
            .expect("a bind off loopback must demand a token");

        assert!(
            !minted.trim().is_empty(),
            "{stored:?}: a blank is not a token"
        );
        assert_eq!(store.persisted_key().as_deref(), Some(minted.as_str()));
        assert_eq!(store.writes(), 1, "{stored:?}: written exactly once");

        let again = resolve_daemon_api_key("0.0.0.0", &repo(&store)).await;
        assert_eq!(again.as_deref(), Some(minted.as_str()), "{stored:?}");
        assert_eq!(
            store.writes(),
            1,
            "{stored:?}: a stored key is not rewritten"
        );
    }
}

/// The daemon asks about the bind first: on loopback it takes no token even
/// with one stored, where the proxy would honour the stored key.
#[tokio::test]
async fn a_loopback_daemon_takes_no_token_whatever_is_stored() {
    for stored in [None, Some("set-by-remote-enable")] {
        let store = Recording::with_key(stored);

        let key = resolve_daemon_api_key("127.0.0.1", &repo(&store)).await;

        assert_eq!(key, None, "{stored:?}");
        assert_eq!(
            store.writes(),
            0,
            "{stored:?}: a loopback bind writes nothing"
        );
        assert_eq!(store.persisted_key().as_deref(), stored);
    }
}

/// A stored key is named, with the command that prints it, and never
/// printed: the banner is in a terminal, and terminal output gets pasted.
#[test]
fn the_banner_never_prints_a_stored_key() {
    for source in [ApiKeySource::Settings, ApiKeySource::Generated] {
        let text = api_key_banner("k3y-5f3a9c", source).join("\n");

        assert!(!text.contains("k3y-5f3a9c"), "{source:?}: {text}");
        assert!(
            text.contains("`gglib config settings reveal proxy-api-key`"),
            "{source:?} must name the reveal: {text}"
        );
    }
}

/// A key that could not be stored exists nowhere else, so it is printed.
#[test]
fn the_banner_prints_a_key_that_was_not_stored() {
    let text = api_key_banner("k3y-5f3a9c", ApiKeySource::GeneratedUnsaved).join("\n");

    assert!(text.contains("Authorization: Bearer k3y-5f3a9c"), "{text}");
    assert!(text.contains("NOT saved"), "{text}");
}
