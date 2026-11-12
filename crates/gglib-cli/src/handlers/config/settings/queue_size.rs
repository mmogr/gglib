//! The download queue size a command stored, told to the running daemon.
//!
//! The daemon's download manager holds the queue size it was given, where
//! the daemon reads its other settings from the store as it uses them. A
//! command that changed the size sends the daemon that serves this library
//! the size now stored, which it hands its queue as the app's write does.
//! That daemon is found as `LibraryChanges::tell_daemon` finds it: by the
//! token it left under this data root.

use crate::daemon_client::{auth, running};

/// Tell the daemon that serves this library that the stored download queue
/// size is now `stored`, `None` being unset, and answer the warning to print
/// when it could not be told.
///
/// The size is stored either way, and a daemon takes it when it starts, so
/// nothing here fails the command. With no daemon, or one that serves
/// another library (it refuses this data root's token), nobody is asked or
/// warned.
pub(super) async fn tell_daemon(stored: Option<u32>) -> Option<String> {
    // With no token here no daemon serves this library, and whatever holds
    // the daemon's port is asked nothing.
    auth::daemon_token()?;
    let daemon = running().await.ok()?;
    match daemon.set_download_queue_size(stored).await {
        Ok(_) => None,
        Err(e) => Some(format!(
            "⚠ The running daemon was not told the download queue size now stored ({e:#}). \
             It takes the stored size when it next starts."
        )),
    }
}

#[cfg(test)]
#[path = "queue_size_tests.rs"]
mod tests;
