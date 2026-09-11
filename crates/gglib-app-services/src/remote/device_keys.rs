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

use super::RemoteOps;
use crate::error::GuiError;

/// Put the stored devices on a listener that is about to be installed.
///
/// Called under `arm`'s install guard, and that placement is what makes a
/// `forget` racing an arm come out right. `forget` tells the edge only when
/// the serve slot is *full*, and until the install it is a reservation — so a
/// seed outside that guard could put back a key whose owner had just been
/// forgotten, leaving an id admitted at the edge that no roster row accounts
/// for. Under the guard there are two orders and both are correct: a `forget`
/// that finished first is already out of the file this reads, and one that
/// has not started finds a full slot and a handle to tell.
///
/// The file is re-read under `RemoteOps::roster`, which `forget` holds across
/// both its writes, so this cannot see a half-applied one. `earlier` is the
/// snapshot `arm` took before its point of no return, used when the re-read
/// fails: that file was parseable a moment ago and both writers replace it
/// atomically, so falling back is a slightly staler roster rather than a
/// wrong one — and the alternative is unwinding a tunnel that is already up.
pub(super) async fn seed(ops: &RemoteOps, handle: &modelpipe::ServeHandle, earlier: DeviceKeys) {
    let keys = {
        let _guard = ops.roster.lock().await;
        read_keys().unwrap_or_else(|e| {
            warn!("re-reading the device keys failed, seeding the earlier read: {e}");
            earlier
        })
    };
    seed_into(handle, keys);
}

/// Put every device this machine knows back on a freshly armed listener.
///
/// Under `TokenPolicy::Named` the listener starts closed: until this runs,
/// nothing admits but a live grant. That is why [`read_keys`] is called
/// *before* the arm reaches its point of no return and this is called after —
/// an unreadable key file must fail the enable while failing it is still
/// free, and this half cannot fail at all.
///
/// A single row that the edge refuses is logged and skipped rather than
/// failing the arm: one hand-edited id should not lock every other device
/// out. The roster and the listener can then disagree, which the device list
/// shows as a row that is not admitted.
pub(super) fn seed_into(handle: &modelpipe::ServeHandle, keys: DeviceKeys) {
    let held = keys.len();
    let mut seeded = 0usize;
    for (id, key) in keys {
        match handle.add_token(&id, key) {
            Ok(()) => seeded += 1,
            Err(e) => warn!(device = %id, "a stored device key was refused by the tunnel: {e}"),
        }
    }
    info!(
        devices = seeded,
        refused = held - seeded,
        "seeded the tunnel with stored device keys"
    );
}

/// Every device key this machine holds. A file that is not there is no
/// devices, not an error: that is a machine that has never invited one.
///
/// A parse failure is *not* softened into an empty map. That would arm a
/// listener admitting nobody while the roster in settings still lists
/// devices, and the operator would see rows and refusals at the same time
/// with nothing saying why.
///
/// # Errors
///
/// `Internal` when the file exists and cannot be read or parsed.
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
