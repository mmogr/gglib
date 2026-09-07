//! Settling the proxy's bearer token at bind time.
//!
//! Split from the supervisor for the file-size gate; the rule it encodes is
//! the supervisor's and is documented on the function.

use std::sync::Arc;

use gglib_core::ApiKeySource;
use gglib_core::ports::SettingsRepository;
use tracing::{info, warn};

/// Settle the bearer token for a proxy about to bind `host`.
///
/// Precedence is flag/env → stored setting → generated. The generated case is
/// deliberately conditional on the bind: a loopback endpoint is already
/// reachable only by processes on this machine, so demanding a token there
/// would be ceremony that breaks every existing local setup for no gain.
/// Binding anywhere else puts the endpoint — and the MCP gateway's filesystem
/// tools — on a network, and that is worth a token the operator did not have
/// to remember to ask for.
///
/// A minted token is persisted rather than kept for the process: a client
/// configured once should keep working across restarts, and a token that
/// changed every launch would train people to turn the feature off.
pub(super) async fn resolve_api_key(
    configured: Option<String>,
    host: &str,
    settings_repo: &Arc<dyn SettingsRepository>,
) -> (Option<String>, ApiKeySource) {
    if let Some(key) = configured {
        return (Some(key), ApiKeySource::Flag);
    }

    let stored = settings_repo
        .load()
        .await
        .inspect_err(|e| warn!("could not read settings while resolving the proxy API key: {e}"))
        .ok();

    if let Some(key) = stored
        .as_ref()
        .and_then(|s| s.proxy_api_key.clone())
        .filter(|key| !key.trim().is_empty())
    {
        return (Some(key), ApiKeySource::Settings);
    }

    if gglib_core::access::is_loopback_host(host) {
        return (None, ApiKeySource::None);
    }

    let key = gglib_core::access::generate_api_key();

    // Only write back settings we successfully read. Saving a `Settings`
    // reconstructed from defaults after a failed load would silently clear
    // every other stored preference to buy one field.
    match stored {
        Some(mut settings) => {
            settings.proxy_api_key = Some(key.clone());
            match settings_repo.save(&settings).await {
                Ok(()) => info!("generated an API key for the non-loopback bind and saved it"),
                // Still guard this run. Refusing to start would be worse, and an
                // unsaved key beats an open endpoint on a network — the banner
                // prints it either way, so the operator can copy it.
                Err(e) => warn!("generated an API key but could not save it: {e}"),
            }
        }
        None => warn!("generated an API key but settings were unreadable, so it was not saved"),
    }

    (Some(key), ApiKeySource::Generated)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use gglib_core::Settings;
    use gglib_core::ports::RepositoryError;

    use super::*;

    /// A settings store that records what was written to it, so a test can
    /// ask whether the key the caller was handed is also the key the next
    /// bind will find.
    struct Recording {
        stored: Mutex<Settings>,
        saves: Mutex<usize>,
    }

    impl Recording {
        fn with_key(key: Option<&str>) -> Arc<Self> {
            let mut settings = Settings::with_defaults();
            settings.proxy_api_key = key.map(str::to_owned);
            Arc::new(Self {
                stored: Mutex::new(settings),
                saves: Mutex::new(0),
            })
        }

        fn persisted_key(&self) -> Option<String> {
            self.stored.lock().unwrap().proxy_api_key.clone()
        }

        fn saves(&self) -> usize {
            *self.saves.lock().unwrap()
        }
    }

    #[async_trait::async_trait]
    impl SettingsRepository for Recording {
        async fn load(&self) -> Result<Settings, RepositoryError> {
            Ok(self.stored.lock().unwrap().clone())
        }
        async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
            *self.stored.lock().unwrap() = settings.clone();
            *self.saves.lock().unwrap() += 1;
            Ok(())
        }
    }

    fn repo(store: &Arc<Recording>) -> Arc<dyn SettingsRepository> {
        Arc::clone(store) as Arc<dyn SettingsRepository>
    }

    /// The default local bind, and the reason `BearerPolicy::tracking` sees
    /// `None` as its floor: a loopback proxy demands nothing, so clearing the
    /// stored key reopens it. Nothing is written either — an operator who has
    /// just run `unset` must not find a key back in settings.
    #[tokio::test]
    async fn a_loopback_bind_asks_for_no_token_and_stores_none() {
        for host in ["127.0.0.1", "localhost", "::1", "127.0.0.1:8080"] {
            let store = Recording::with_key(None);
            let (key, source) = resolve_api_key(None, host, &repo(&store)).await;

            assert_eq!(key, None, "{host} should bind open");
            assert_eq!(source, ApiKeySource::None, "{host} should bind open");
            assert_eq!(
                store.persisted_key(),
                None,
                "{host}: nothing may be written back into a setting just cleared"
            );
            assert_eq!(store.saves(), 0, "{host}: a loopback bind saves nothing");
        }
    }

    /// The case `docs/remote.md`'s unset-then-rebind procedure does *not*
    /// cover. Off loopback the rebind mints a fresh key **and writes it into
    /// `proxy_api_key`**, so the clear the operator just performed is undone
    /// and the proxy comes back closed with a credential nobody has seen.
    #[tokio::test]
    async fn a_non_loopback_bind_mints_a_token_and_writes_it_back() {
        for host in ["0.0.0.0", "::", "192.168.1.5", "gglib.lan"] {
            let store = Recording::with_key(None);
            let (key, source) = resolve_api_key(None, host, &repo(&store)).await;

            let minted = key.expect("a bind off loopback must demand a token");
            assert!(!minted.trim().is_empty(), "{host}: a blank is not a token");
            assert_eq!(source, ApiKeySource::Generated, "{host}");
            assert_eq!(
                store.persisted_key().as_deref(),
                Some(minted.as_str()),
                "{host}: the minted key is persisted, so the clear did not stick"
            );
            assert_eq!(store.saves(), 1, "{host}: written exactly once");
        }
    }

    /// Precedence, and the ordering that carries it: the stored key is
    /// consulted *before* the bind host is. Ask about the host first and a
    /// loopback proxy would ignore a key the operator deliberately set.
    #[tokio::test]
    async fn a_stored_key_is_honoured_on_a_loopback_bind() {
        let store = Recording::with_key(Some("set-by-remote-enable"));
        let (key, source) = resolve_api_key(None, "127.0.0.1", &repo(&store)).await;

        assert_eq!(key.as_deref(), Some("set-by-remote-enable"));
        assert_eq!(source, ApiKeySource::Settings);
        assert_eq!(store.saves(), 0, "reading a stored key rewrites nothing");
    }

    /// And a flag outranks the store, which is why `--api-key` cannot be
    /// silently replaced by whatever a settings write puts there.
    #[tokio::test]
    async fn a_configured_key_outranks_both_the_store_and_the_host() {
        let store = Recording::with_key(Some("stored"));
        let (key, source) =
            resolve_api_key(Some("from-the-flag".into()), "0.0.0.0", &repo(&store)).await;

        assert_eq!(key.as_deref(), Some("from-the-flag"));
        assert_eq!(source, ApiKeySource::Flag);
        assert_eq!(store.persisted_key().as_deref(), Some("stored"));
    }
}
