//! The keys this machine issued to paired devices, on disk.
//!
//! Deliberately *not* in `settings_kv` beside `proxy_api_key`. That store is
//! printed in full by `gglib config settings show` — unmasked on purpose, so a
//! rotated key can be recovered — and that output is what people paste into
//! bug reports. One shared key there is a known cost; a device key each would
//! quietly undo what per-device revocation is for. The roster's readable half
//! (ids, labels, last-seen) stays in settings; only the secrets are here.
//!
//! Same directory and same posture as the endpoint identity: `0600`, under
//! `data/`, which a debug build resolves to the repository checkout where
//! `.gitignore` covers it.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::private_file::write_private;
use crate::paths::{PathError, remote_identity_location, remote_identity_path};

/// Every device key this machine holds, by the id the tunnel edge knows it as.
pub type DeviceKeys = BTreeMap<String, String>;

/// Where the keys live: beside the endpoint identity.
///
/// # Errors
///
/// Whatever resolving the data root returns.
pub fn device_keys_path() -> Result<PathBuf, PathError> {
    Ok(remote_identity_path()?.with_file_name(KEYS_FILE))
}

/// The file [`device_keys_path`] names, with nothing under the data root
/// created or tightened on the way, for a caller that only reads.
///
/// # Errors
///
/// Whatever resolving the data root returns.
pub fn device_keys_location() -> Result<PathBuf, PathError> {
    Ok(remote_identity_location()?.with_file_name(KEYS_FILE))
}

/// The keys' file name, beside the endpoint identity.
const KEYS_FILE: &str = "remote_devices";

/// Read the roster's keys, or an empty map when nothing has been issued.
///
/// A missing file is the empty map rather than an error: a machine that has
/// never invited anything is not a machine in a bad state.
///
/// # Errors
///
/// [`io::Error`] when the file exists and cannot be read or parsed. A parse
/// failure is *not* softened into an empty map — that would arm a listener
/// admitting nobody while the roster in settings says otherwise, and the
/// operator would see devices listed and refused at the same time.
pub fn load(path: &Path) -> io::Result<DeviceKeys> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(DeviceKeys::new()),
        Err(e) => Err(e),
    }
}

/// Replace the stored keys, `0600`, atomically; [`write_private`] says how.
///
/// # Errors
///
/// [`io::Error`] from serializing the keys or from [`write_private`].
pub fn store(path: &Path, keys: &DeviceKeys) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(keys).map_err(io::Error::other)?;
    write_private(path, &json)
}

#[cfg(test)]
#[path = "device_keys_tests.rs"]
mod device_keys_tests;
