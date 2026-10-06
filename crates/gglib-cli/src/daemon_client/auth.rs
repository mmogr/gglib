//! The credential this CLI presents to the daemon's management API.
//!
//! Split from `mod.rs`, which owns the connection, because resolving the token
//! needs the settings store and finding the daemon does not.
//!
//! Every `/api` route takes the daemon's own token, which the daemon mints in
//! a `0600` file at every start, so a CLI running as the owner reads it and
//! needs nothing else. A daemon bound off loopback (`--share-lan`) also takes
//! its key, which `resolve_daemon_api_key` reads or mints and stores as
//! `proxy_api_key`; a CLI that cannot read the file, another account's among
//! them, falls back to that key, and on loopback is refused.
//!
//! The token comes first, because it opens every daemon, loopback and
//! `--share-lan` alike. `GGLIB_API_KEY` is the proxy's key (`shared_args`
//! recommends it in `.env`), which a loopback daemon never takes, so it is
//! sent only where there is no token to read, ahead of the stored key.

use std::future::Future;
use std::path::{Path, PathBuf};

use gglib_core::access::{daemon_token_path, read_daemon_token};
use gglib_core::contracts::http::daemon::DAEMON_TOKEN_REQUIRED_TYPE;

use crate::bootstrap::CliContext;

/// The environment variable an operator uses to override the stored key.
///
/// `gglib proxy` subcommands take it as `--api-key`'s `env =` source, but the
/// daemon-facing commands have no such flag, so it is read directly here. It
/// outranks the stored key, as `shared_args` documents, and not the daemon's
/// token, which it is not.
const API_KEY_ENV: &str = "GGLIB_API_KEY";

/// The credential to present to the daemon; [`Local::credential`] says which.
pub(crate) async fn daemon_api_key(ctx: &CliContext) -> Option<String> {
    Local::here().credential(proxy_key(ctx, None)).await
}

/// The daemon's token, when there is one this account can read.
pub(crate) fn daemon_token() -> Option<String> {
    token_at(&daemon_token_path().ok()?)
}

/// Where this machine's credentials for its own daemon are: the daemon's token
/// file and the operator's key. A value, so a test can name its own.
pub(crate) struct Local {
    /// `GGLIB_API_KEY`, when it is set and not blank.
    pub(crate) env: Option<String>,
    /// The daemon's token file, when the data root resolves.
    pub(crate) token_path: Option<PathBuf>,
}

impl Local {
    /// This process's environment and this machine's data root.
    pub(crate) fn here() -> Self {
        Self {
            env: std::env::var(API_KEY_ENV)
                .ok()
                .filter(|key| !key.trim().is_empty()),
            token_path: daemon_token_path().ok(),
        }
    }

    /// The daemon's token first, then the operator's key, then `stored`,
    /// which is only awaited when neither is there. Read at each call: the
    /// daemon mints a new token at every start.
    pub(crate) async fn credential(
        &self,
        stored: impl Future<Output = Option<String>>,
    ) -> Option<String> {
        if let Some(token) = self.token_path.as_deref().and_then(token_at) {
            return Some(token);
        }
        if let Some(key) = &self.env {
            return Some(key.clone());
        }
        stored.await
    }
}

/// The token in the file at `path`, when it can be read, is not blank, and
/// is open to nobody else ([`read_daemon_token`] refuses one that is).
fn token_at(path: &Path) -> Option<String> {
    read_daemon_token(path)
        .ok()
        .flatten()
        .map(|token| token.as_str().to_owned())
}

/// The key a proxy on this machine is sent: `flag` when one was given, and
/// otherwise the key stored as `proxy_api_key`, when one is and it is not
/// blank. The daemon is presented the same stored key when there is neither a
/// token nor an operator's key ([`Local::credential`]).
///
/// An unreadable settings store yields `None` rather than an error: failing
/// the command outright would turn a maybe-irrelevant local problem into a
/// hard stop. A server that wants a key answers 401 with a message that names
/// the remedy.
pub(crate) async fn proxy_key(ctx: &CliContext, flag: Option<String>) -> Option<String> {
    if flag.is_some() {
        return flag;
    }
    ctx.app
        .settings()
        .get()
        .await
        .ok()
        .and_then(|settings| settings.proxy_api_key)
        .filter(|key| !key.trim().is_empty())
}

/// What to tell someone whose daemon call came back 401.
///
/// Worth spelling out because the failure is confusing by construction:
/// `/health` sits outside the bearer layer, so the daemon is found and looks
/// healthy, and only the call after it fails.
pub(crate) fn unauthorized_hint() -> String {
    format!(
        "the daemon requires an API key. It is stored as `proxy-api-key` \
         (`gglib config settings show`); set {API_KEY_ENV} to override it for one run."
    )
}

/// What to tell someone whose daemon call came back 401 with `body`: the
/// daemon's own sentence when a route wanted its token, which says how to get
/// it, and [`unauthorized_hint`] otherwise.
pub(crate) fn unauthorized(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some(DAEMON_TOKEN_REQUIRED_TYPE))
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_else(unauthorized_hint)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod auth_tests;
