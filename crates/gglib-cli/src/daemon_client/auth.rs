//! The credential this CLI presents to the daemon's management API.
//!
//! Split from `mod.rs`, which owns the connection, because resolving the token
//! needs the data root and finding the daemon does not.
//!
//! Every `/api` route takes the daemon's own token and nothing else. The
//! daemon mints it in a `0600` file at every start, so a CLI running as the
//! owner reads it. A CLI that cannot read the file, another account's among
//! them, presents nothing and is refused.

use std::path::{Path, PathBuf};

use gglib_core::access::{daemon_token_path, read_daemon_token};
use gglib_core::contracts::http::daemon::{
    DAEMON_TOKEN_REQUIRED_MESSAGE, DAEMON_TOKEN_REQUIRED_TYPE,
};
use gglib_core::secret::held_key;

use crate::bootstrap::CliContext;

/// The daemon's token, when there is one this account can read.
pub(crate) fn daemon_token() -> Option<String> {
    Local::here().token()
}

/// Where this machine's daemon leaves its token. A value, so a test can name
/// its own.
pub(crate) struct Local {
    /// The daemon's token file, when the data root resolves.
    pub(crate) token_path: Option<PathBuf>,
}

impl Local {
    /// This machine's data root.
    pub(crate) fn here() -> Self {
        Self {
            token_path: daemon_token_path().ok(),
        }
    }

    /// The daemon's token, read at each call: the daemon mints a new one at
    /// every start.
    pub(crate) fn token(&self) -> Option<String> {
        self.token_path.as_deref().and_then(token_at)
    }
}

/// The token in the file at `path`, when it can be read, is not blank, and
/// is open to nobody else ([`read_daemon_token`] refuses one that is).
fn token_at(path: &Path) -> Option<String> {
    #[cfg(test)]
    if let Ok(left) = STAND_IN_TOKEN.try_with(Clone::clone) {
        return left;
    }
    read_daemon_token(path)
        .ok()
        .flatten()
        .map(|token| token.as_str().to_owned())
}

#[cfg(test)]
tokio::task_local! {
    /// In a test, the token a stand-in daemon left, or `None` for one that
    /// left none. A task that sets it reads that in place of any token
    /// file: the one under the data root is one file for every test in the
    /// binary.
    pub(crate) static STAND_IN_TOKEN: Option<String>;
}

/// The key a proxy on this machine is sent: `flag` when one was given, and
/// otherwise the key stored as `proxy_api_key`, when one is and it is not
/// blank. The daemon is never sent it.
///
/// An unreadable settings store yields `None` rather than an error: failing
/// the command outright would turn a maybe-irrelevant local problem into a
/// hard stop. A server that wants a key answers 401 with a message that names
/// the remedy.
pub(crate) async fn proxy_key(ctx: &CliContext, flag: Option<String>) -> Option<String> {
    if flag.is_some() {
        return flag;
    }
    let settings = ctx.app.settings().get().await.ok()?;
    held_key(settings.proxy_api_key.as_deref()).map(str::to_owned)
}

/// What to tell someone whose daemon call came back 401 with `body`: the
/// daemon's own sentence, which says how to get its token. That refusal is
/// the daemon's only 401, so a body that is not it gets the same sentence.
///
/// Worth spelling out because the failure is confusing by construction:
/// `/health` sits outside the bearer layer, so the daemon is found and looks
/// healthy, and only the call after it fails.
///
/// Under `GGLIB_DATA_DIR` the likelier cause is named as well: the token this
/// command sent is its own data root's, and the daemon on the port belongs to
/// another one.
pub(crate) fn unauthorized(body: &str) -> String {
    unauthorized_under(body, std::env::var_os("GGLIB_DATA_DIR").is_some())
}

/// [`unauthorized`], told whether `GGLIB_DATA_DIR` is set.
fn unauthorized_under(body: &str, own_data_root: bool) -> String {
    let sentence = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some(DAEMON_TOKEN_REQUIRED_TYPE))
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_else(|| DAEMON_TOKEN_REQUIRED_MESSAGE.to_owned());
    if own_data_root {
        other_data_root(&sentence)
    } else {
        sentence
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod auth_tests;

/// The daemon's `sentence`, and what it most likely means for a command run
/// under `GGLIB_DATA_DIR`.
fn other_data_root(sentence: &str) -> String {
    format!(
        "{sentence}. GGLIB_DATA_DIR is set for this command, so the daemon on port {} is \
         probably another data folder's: set GGLIB_DAEMON_PORT as well, to run one of its own.",
        gglib_core::daemon_port()
    )
}
