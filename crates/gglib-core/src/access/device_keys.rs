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
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::paths::{PathError, remote_identity_path};

/// Every device key this machine holds, by the id the tunnel edge knows it as.
pub type DeviceKeys = BTreeMap<String, String>;

/// Where the keys live: beside the endpoint identity.
///
/// # Errors
///
/// Whatever resolving the data root returns.
pub fn device_keys_path() -> Result<PathBuf, PathError> {
    Ok(remote_identity_path()?.with_file_name("remote_devices"))
}

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

/// Replace the stored keys, `0600`, atomically.
///
/// Written to a sibling temporary file and renamed, so a crash mid-write
/// leaves the previous roster rather than a truncated one: a half-written file
/// is a listener that admits some devices and not others, with nothing saying
/// which.
///
/// The temporary file is `0600` from the moment it exists, not from a chmod
/// once the keys are already in it; `create_private` says why. A temporary a
/// crash leaves behind is therefore no more readable than the file it would
/// have replaced, and it is not swept here: another process may be mid-write
/// on a temporary of its own, and deleting that one brings back the rename
/// collision the per-writer names below exist to prevent.
///
/// **The temporary file is named per writer, not per path.** A fixed
/// `.tmp` sibling makes two concurrent writers collide on one filename:
/// both write it, the first renames it away, and the second fails at its own
/// `rename` with `NotFound` — an error raised for a write that was perfectly
/// valid. The rename is what makes this atomic, and it only does so if each
/// writer has its own thing to rename.
///
/// # Errors
///
/// [`io::Error`] from creating the directory, creating or writing the
/// temporary file, setting its mode, or the rename.
pub fn store(path: &Path, keys: &DeviceKeys) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(keys).map_err(io::Error::other)?;

    let tmp = path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        NEXT_TMP.fetch_add(1, Ordering::Relaxed)
    ));
    // `restrict` before a byte is written rather than after: the mode asked of
    // `open` only applies to a file it creates, and a leftover under this name
    // — a recycled pid after a reboot starts the counter again — keeps the
    // mode it was born with.
    let written = create_private(&tmp)
        .and_then(|mut file| {
            restrict(&tmp)?;
            file.write_all(&json)
        })
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        // Best effort: a temporary nobody renamed is litter beside a `0600`
        // secret, and the error being returned is the one that matters.
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// Distinguishes one writer's temporary file from another's within a process;
/// the pid does it across processes.
static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

/// Open the temporary file for writing, `0600` from the moment it exists.
///
/// `fs::write` creates with `0666` less the umask, which is `0644` on most
/// machines, and a mode set afterwards leaves a window in which every device
/// key is on disk and readable by anyone on the machine. A crash inside that
/// window leaves them that way for good, under a name nothing goes back to.
/// Asking `open` for the mode closes the window; it is how modelpipe creates
/// the endpoint identity beside this file.
///
/// The mode only applies to a file this call creates. A leftover under the
/// same name keeps the mode it has, which is why `store` still calls
/// `restrict` before it writes.
#[cfg(unix)]
fn create_private(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

/// Windows has no mode to ask for, so this is the plain create `fs::write`
/// always did; `restrict` says what protects the file there.
#[cfg(not(unix))]
fn create_private(path: &Path) -> io::Result<fs::File> {
    fs::File::create(path)
}

/// `0600` where the platform has a notion of it.
#[cfg(unix)]
fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

/// Windows has no mode to set; the file inherits the directory's ACL, which is
/// the same protection the endpoint identity gets there.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)] // the Unix twin really can fail
fn restrict(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "device_keys_tests.rs"]
mod device_keys_tests;
