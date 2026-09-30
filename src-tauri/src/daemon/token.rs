//! The daemon's token, which every request the app's Rust code sends carries.
//!
//! The daemon mints it in a `0600` file beside the device keys at every start,
//! and the app runs as the same account on the same data directory, so it can
//! read it. Every `/api` route asks for it (#983).

use gglib_core::access::{daemon_token_path, read_daemon_token};

use super::{Daemon, base_url};

/// The daemon's token, or `None` when there is none this account can read.
///
/// Read at every call rather than once: the daemon mints a new one each time
/// it starts, from the tray's Start gglib Service among others.
pub(crate) fn daemon_token() -> Option<String> {
    let token = read_daemon_token(&daemon_token_path().ok()?).ok()??;
    Some(token.as_str().to_owned())
}

impl Daemon {
    /// A request to the daemon carrying its token, as the CLI's
    /// `DaemonHandle::request` does, so no call site decides for itself.
    pub(super) fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.request(method, format!("{}{path}", base_url()));
        match daemon_token() {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}
