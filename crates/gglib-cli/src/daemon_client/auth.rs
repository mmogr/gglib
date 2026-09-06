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

use crate::bootstrap::CliContext;

/// The environment variable an operator uses to override the stored token.
///
/// `gglib proxy` subcommands take it as `--api-key`'s `env =` source, but the
/// daemon-facing commands have no such flag, so it is read directly here. The
/// precedence is the one `shared_args` documents: a key supplied by the
/// operator outranks the stored setting.
const API_KEY_ENV: &str = "GGLIB_API_KEY";

/// The token to present to the daemon, or `None` when it wants none.
///
/// An unreadable settings store yields `None` rather than an error, matching
/// `resolve_client_api_key`'s reasoning for the proxy: the daemon is very
/// likely unauthenticated, and failing the command outright would turn a
/// maybe-irrelevant local problem into a hard stop. A daemon that *does* want
/// a token answers 401 with a message that names the remedy.
pub(crate) async fn daemon_api_key(ctx: &CliContext) -> Option<String> {
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
