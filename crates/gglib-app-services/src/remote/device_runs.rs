//! A forgotten device's runs: they go once its key is out of the file.
//!
//! A run holds a reply only the device that started it may read, and a
//! device with no key can no longer ask for it. So `forget` drops them once
//! the key is gone, and still does when a later step of `forget` fails: a
//! roster write that fails leaves a row, not a device that can reach them.

use tracing::info;

use super::RemoteOps;
use super::device_keys::read_keys;

impl RemoteOps {
    /// Cancel and drop `device`'s runs, when the key file no longer holds
    /// its key. A file that cannot be read keeps them: nothing says the key
    /// is gone.
    pub(super) fn drop_runs_once_unkeyed(&self, device: &str) {
        if !read_keys(self).is_ok_and(|keys| !keys.contains_key(device)) {
            return;
        }
        let dropped = self
            .proxy
            .runs()
            .map_or(0, |runs| runs.forget_device(device));
        if dropped > 0 {
            info!(device = %device, dropped, "dropped a forgotten device's runs");
        }
    }
}

#[cfg(test)]
#[path = "forget_runs_tests.rs"]
mod forget_runs_tests;
