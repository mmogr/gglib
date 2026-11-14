//! The download queue size, told to the daemon on a [`DaemonHandle`].
//!
//! A `gglib config set`, `unset` or `reset` stores settings in its own process.
//! The daemon reads most of them from the store as it uses them, but its
//! download manager holds the queue size it was given, so a command that
//! changed the size sends it the size now stored.

use std::time::Duration;

use anyhow::Result;

use super::{DaemonHandle, paths};

impl DaemonHandle {
    /// Have the daemon store `size` as the download queue size, `None`
    /// clearing it, through the settings route the app writes with, which
    /// hands its download manager the size now stored.
    ///
    /// `false` is its 401: the token this data root holds is not the one it
    /// minted, so it serves another library and its queue is not this one's.
    pub(crate) async fn set_download_queue_size(&self, size: Option<u32>) -> Result<bool> {
        let body = serde_json::json!({ paths::SETTINGS_QUEUE_SIZE_FIELD: size });
        let response = self
            .request(reqwest::Method::PUT, paths::SETTINGS_PATH)
            .json(&body)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(false);
        }
        Self::expect_ok(response).await?;
        Ok(true)
    }
}
