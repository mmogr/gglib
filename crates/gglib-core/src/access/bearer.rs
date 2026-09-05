//! Matching an `Authorization` header against the configured bearer token.
//!
//! Split from `mod.rs` rather than added to it because that file is near its
//! complexity budget, and because this is one self-contained decision: given
//! what a client sent and what the endpoint expects, does the request get in.

use std::sync::Arc;

use super::constant_time_eq;
use crate::services::SettingsCache;

/// The auth scheme this endpoint speaks, compared case-insensitively.
const BEARER: &str = "bearer";

/// Whether `presented` — the raw `Authorization` header value, or `None` when
/// the client sent none — carries `expected_key`.
///
/// # Why the scheme is matched case-insensitively
///
/// RFC 9110 §11.1 defines the auth scheme as a `token`, and tokens are
/// case-insensitive. `bearer sk-…` is therefore a correct request, and the
/// previous comparison — the whole `"Bearer <key>"` string, byte for byte —
/// answered it with a 401 that said the key was wrong. It was not; only its
/// capitalisation was, and nothing in the response said so. That is the least
/// actionable rejection available, and it matters here beyond pedantry: the
/// tunnel in front of this endpoint accepts the same header the RFC does, so
/// two doors checking one credential disagreed about whether it was valid.
///
/// The scheme comparison is deliberately **not** constant-time. It is a public
/// protocol keyword, not a secret, and there is nothing to leak by returning
/// early on it. Only the credential goes to [`constant_time_eq`].
///
/// # What is still refused
///
/// Everything a lenient comparison would wave through. A different scheme
/// (`Basic <key>`), the bare key with no scheme at all, a prefix of the key,
/// and an empty credential are all rejected. The last of those is load-bearing:
/// settings validation refuses a blank `proxy_api_key` precisely so that
/// `Bearer ` cannot become a credential everyone holds, and splitting the
/// header on its space must not reintroduce that from the other side.
///
/// Whitespace follows the grammar rather than being trimmed indiscriminately:
/// the scheme and the credential are separated by one or more spaces
/// (`1*SP`), and trailing optional whitespace is not part of the credential.
#[must_use]
pub fn bearer_matches(presented: Option<&str>, expected_key: &str) -> bool {
    // A blank expectation can never be satisfied. Unreachable through the
    // settings path, which rejects one, but this function is the last place
    // that assumption could go wrong quietly rather than loudly.
    if expected_key.is_empty() {
        return false;
    }

    let Some(header) = presented else {
        return false;
    };

    // No space means no credential — `"Bearer"` alone, or a bare key sent with
    // the scheme omitted, both land here.
    let Some((scheme, rest)) = header.split_once(' ') else {
        return false;
    };

    if !scheme.eq_ignore_ascii_case(BEARER) {
        return false;
    }

    let credential = rest.trim();
    if credential.is_empty() {
        return false;
    }

    constant_time_eq(credential.as_bytes(), expected_key.as_bytes())
}

/// Which token a running endpoint currently requires.
///
/// # Why this is not just a string
///
/// The expected token used to be resolved once, at bind, and baked into the
/// middleware — so a key rotated afterwards was never honoured and a key set
/// afterwards was never enforced. Worse, the guard was only *installed* when a
/// token existed at bind, so an endpoint that started open could not be closed
/// without a restart.
///
/// The fix cannot be an in-process notification. `gglib config settings set`
/// writes the database from a **separate process**, so nothing the daemon
/// subscribes to would ever see it — the same reasoning
/// [`SettingsCache`] already records for every
/// other setting. Reading through that cache is what makes a rotation take
/// effect here at all.
///
/// **The staleness is bounded, not zero.** A revoked key keeps working for up
/// to [`SETTINGS_CACHE_TTL`](crate::services::SETTINGS_CACHE_TTL). That is the
/// accepted trade, and it is strictly better than what it replaces, where a
/// rotation performed through the CLI never took effect at all.
#[derive(Clone)]
pub struct BearerPolicy {
    /// A token supplied by flag or environment. It does not live in settings,
    /// so nothing in settings may override it.
    pinned: Option<Arc<str>>,
    /// The token in force at bind, kept as a floor.
    floor: Option<Arc<str>>,
    /// The live view of the stored token.
    settings: Option<Arc<SettingsCache>>,
}

impl BearerPolicy {
    /// A token the operator supplied directly, which never tracks settings.
    ///
    /// `--api-key` and `GGLIB_API_KEY` outrank the stored value by design, so
    /// letting a settings write replace one would both invert that precedence
    /// and lock out the operator who passed it.
    #[must_use]
    pub fn pinned(key: &str) -> Self {
        Self {
            pinned: Some(Arc::from(key)),
            floor: None,
            settings: None,
        }
    }

    /// A token read from settings — or absent — which tracks later writes.
    ///
    /// `bind_key` is whatever was in force when the endpoint bound, and is
    /// kept as a floor: if the stored value later disappears, this endpoint
    /// keeps demanding the token it started with rather than falling open.
    /// **Authentication can be switched on at runtime and never off**, which
    /// is the asymmetry a listener bound off loopback needs — clearing the
    /// setting must not silently expose it.
    #[must_use]
    pub fn tracking(bind_key: Option<&str>, settings: Arc<SettingsCache>) -> Self {
        Self {
            pinned: None,
            floor: bind_key.map(Arc::from),
            settings: Some(settings),
        }
    }

    /// A token that can never change and is never required. For hosts with no
    /// settings to read, such as tests and embedded servers.
    #[must_use]
    pub fn fixed(key: Option<&str>) -> Self {
        Self {
            pinned: key.map(Arc::from),
            floor: None,
            settings: None,
        }
    }

    /// The token a request must present right now, or `None` while this
    /// endpoint is unauthenticated.
    pub async fn current(&self) -> Option<Arc<str>> {
        if let Some(pinned) = &self.pinned {
            return Some(Arc::clone(pinned));
        }
        let stored = match &self.settings {
            Some(cache) => cache
                .get()
                .await
                .proxy_api_key
                .as_deref()
                .filter(|key| !key.trim().is_empty())
                .map(Arc::from),
            None => None,
        };
        stored.or_else(|| self.floor.clone())
    }

    /// Whether `presented` gets in.
    ///
    /// An endpoint with no token configured admits everyone, which is the
    /// loopback default and the behaviour this had before authentication
    /// existed.
    pub async fn admits(&self, presented: Option<&str>) -> bool {
        self.current()
            .await
            .is_none_or(|expected| bearer_matches(presented, &expected))
    }
}

#[cfg(test)]
#[path = "bearer_tests.rs"]
mod bearer_tests;
