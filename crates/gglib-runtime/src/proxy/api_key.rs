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
