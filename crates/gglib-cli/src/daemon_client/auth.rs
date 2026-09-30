//! The credential this CLI presents to the daemon's management API.
//!
//! Split from `mod.rs`, which owns the connection, because resolving the token
//! needs the settings store and finding the daemon does not.
//!
//! The daemon on `127.0.0.1:9887` is unauthenticated by default — that is
//! `DaemonAccess::new`'s contract, and the socket is the boundary. A key is
//! only in force when the daemon bound off loopback (`--share-lan`), where
//! `resolve_daemon_api_key` reads or mints one and stores it as
//! `proxy_api_key`. Until this module existed the CLI never sent an
//! `Authorization` header at all, so a `--share-lan` daemon answered 401 to
//! its own CLI on every `/api/*` call.
//!
//! The daemon's own token comes first. The routes that change who is trusted
//! answer nothing else, and every other route takes it too, so a CLI that can
//! read the file needs no key. One that cannot, such as another account's,
//! falls back to the key and is refused those routes.

use std::future::Future;
use std::path::Path;

use gglib_core::access::{daemon_token_path, read_daemon_token};
use gglib_core::contracts::http::daemon::DAEMON_TOKEN_REQUIRED_TYPE;

use crate::bootstrap::CliContext;

/// The environment variable an operator uses to override the stored token.
///
/// `gglib proxy` subcommands take it as `--api-key`'s `env =` source, but the
/// daemon-facing commands have no such flag, so it is read directly here. The
/// precedence is the one `shared_args` documents: a key supplied by the
/// operator outranks the stored setting.
const API_KEY_ENV: &str = "GGLIB_API_KEY";

/// The credential to present to the daemon: its token when this account can
/// read it, else the API key [`api_key`] finds, else `None`.
pub(crate) async fn daemon_api_key(ctx: &CliContext) -> Option<String> {
    let path = daemon_token_path().ok();
    prefer_token(path.as_deref(), api_key(ctx)).await
}

/// The daemon's token, when there is one this account can read.
pub(crate) fn daemon_token() -> Option<String> {
    token_at(&daemon_token_path().ok()?)
}

/// The token in the file at `path`, when it can be read and is not blank.
fn token_at(path: &Path) -> Option<String> {
    read_daemon_token(path)
        .ok()
        .flatten()
        .map(|token| token.as_str().to_owned())
}

/// The token at `path` when there is one, and `fallback` only when not.
async fn prefer_token(
    path: Option<&Path>,
    fallback: impl Future<Output = Option<String>>,
) -> Option<String> {
    if let Some(token) = path.and_then(token_at) {
        return Some(token);
    }
    fallback.await
}

/// The API key to present to the daemon, or `None` when it wants none.
///
/// An unreadable settings store yields `None` rather than an error, matching
/// `resolve_client_api_key`'s reasoning for the proxy: the daemon is very
/// likely unauthenticated, and failing the command outright would turn a
/// maybe-irrelevant local problem into a hard stop. A daemon that *does* want
/// a token answers 401 with a message that names the remedy.
async fn api_key(ctx: &CliContext) -> Option<String> {
    if let Ok(from_env) = std::env::var(API_KEY_ENV)
        && !from_env.trim().is_empty()
    {
        return Some(from_env);
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
