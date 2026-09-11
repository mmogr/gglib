//! The `0600` file that says which key admits which device.
//!
//! The secret half of the two stores `devices.rs` describes — the readable
//! half is the roster in settings, which `roster.rs` owns. Split out because
//! the file is also what a freshly armed listener is seeded from, which is
//! `serve.rs`'s business and not the invite path's, and because `devices.rs`
//! is at its size budget without it.
//!
//! A thin layer over [`gglib_core::access`]: that module owns the format, the
//! mode and the atomic replace; this one owns where the errors go and what
//! they say to a person.

use gglib_core::access::{DeviceKeys, device_keys_path, load_device_keys, store_device_keys};
use tracing::{info, warn};

use crate::error::GuiError;

/// Put every device this machine knows back on a freshly armed listener.
///
/// Under `TokenPolicy::Named` the listener starts closed: until this runs,
/// nothing admits but a live grant. A key file that cannot be read is
/// therefore a tunnel nobody can use, which is the right way round — the
/// alternative is a tunnel that quietly admits fewer devices than the roster
/// claims.
///
/// A single row that the edge refuses is logged and skipped rather than
/// failing the arm: one hand-edited id should not lock every other device
/// out. The roster and the listener can then disagree, which `list` shows as
/// a row that is not admitted.
///
/// # Errors
///
/// `Internal` when the key file exists and cannot be read or parsed.
pub(super) async fn seed(handle: &modelpipe::ServeHandle) -> Result<(), GuiError> {
    let keys = read_keys()?;
    let held = keys.len();
    let seeded = seed_into(handle, keys);
    info!(
        devices = seeded,
        refused = held - seeded,
        "seeded the tunnel with stored device keys"
    );
    Ok(())
}

/// The loop itself, over keys already read — which is the per-row policy, and
/// the only part of [`seed`] a test can reach without a data directory.
///
/// Returns how many the listener took.
fn seed_into(handle: &modelpipe::ServeHandle, keys: DeviceKeys) -> usize {
    let mut seeded = 0usize;
    for (id, key) in keys {
        match handle.add_token(&id, key) {
            Ok(()) => seeded += 1,
            Err(e) => warn!(device = %id, "a stored device key was refused by the tunnel: {e}"),
        }
    }
    seeded
}

/// Every device key this machine holds. A file that is not there is no
/// devices, not an error: that is a machine that has never invited one.
///
/// # Errors
///
/// `Internal` when the file exists and cannot be read or parsed — including
/// when its mode lets anything else read it.
pub(super) fn read_keys() -> Result<DeviceKeys, GuiError> {
    let path = device_keys_path()
        .map_err(|e| GuiError::Internal(format!("could not place the device keys: {e}")))?;
    load_device_keys(&path)
        .map_err(|e| GuiError::Internal(format!("could not read the device keys: {e}")))
}

/// Replace the file with `keys`, atomically and `0600`.
///
/// # Errors
///
/// `Internal` when the directory cannot be resolved or the file cannot be
/// written.
pub(super) fn write_keys(keys: &DeviceKeys) -> Result<(), GuiError> {
    let path = device_keys_path()
        .map_err(|e| GuiError::Internal(format!("could not place the device keys: {e}")))?;
    store_device_keys(&path, keys)
        .map_err(|e| GuiError::Internal(format!("could not write the device keys: {e}")))
}

#[cfg(test)]
#[path = "device_keys_tests.rs"]
mod device_keys_tests;
