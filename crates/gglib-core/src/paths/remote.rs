//! Where the remote tunnel's stored endpoint keys live.
//!
//! `gglib remote enable` writes here every time, and the tunnel reuses what it
//! finds: the endpoint key lasts, so a device pairs once instead of at every
//! restart (ADR 0012: decision 4 and the amendment under it that reverses
//! it). Deleting this file retires this machine's address and revokes
//! no device: the tunnel mints a new key the next time it comes up, and still
//! admits every device key kept beside this one that the roster lists.
//! `gglib remote status` prints the path.
//!
//! The keys this machine joins other machines with go in the directory
//! [`remote_join_dir`] names, one for each machine it joins, so that a machine
//! it joins can see the same endpoint each time this one connects.

use std::path::PathBuf;

use super::error::PathError;
use super::platform::data_root;
use super::private::create_private_dir;

/// Path to the stored iroh endpoint key for the remote tunnel.
///
/// Under `data/` rather than beside `pids/`, and that is load-bearing rather
/// than tidy: a debug build resolves the data root to the repository checkout,
/// and `.gitignore` ignores `/data` — so a private key written here cannot be
/// committed by accident, where one written a level up would sit untracked in
/// the working tree waiting for a `git add -A`.
///
/// The directory is created if it is not there; the file is not. modelpipe
/// mints it `0600` on first use and refuses to read one that others can read.
pub fn remote_identity_path() -> Result<PathBuf, PathError> {
    let data_dir = data_root()?.join("data");

    create_private_dir(&data_dir).map_err(|e| PathError::CreateFailed {
        path: data_dir.clone(),
        reason: e.to_string(),
    })?;

    Ok(data_dir.join("remote_identity"))
}

/// The directory for the endpoint keys this machine joins other machines
/// with, one file for each machine it joins: `<data root>/data/remote_join`.
///
/// Apart from [`remote_identity_path`], the key this machine serves with,
/// because one key is one endpoint and a machine can serve and join at once.
/// Each file is named for the machine it joins, which the caller supplies:
/// this crate does not read tickets.
///
/// Nothing under the data root is created here, so the path can be named
/// without touching what is kept there. A caller about to dial has to make
/// the directory, with [`create_private_dir`], because modelpipe mints the key
/// file and not the directory it sits in.
pub fn remote_join_dir() -> Result<PathBuf, PathError> {
    Ok(data_root()?.join("data").join("remote_join"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};

    #[test]
    fn remote_identity_is_under_the_ignored_data_directory() {
        // A root of its own: `remote_identity_path` makes or tightens
        // `<data root>/data`, which in a debug build is the checkout's (#955).
        let _guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().expect("tempdir");
        let _env = EnvVarGuard::set("GGLIB_DATA_DIR", root.path().to_string_lossy().as_ref());
        let identity = remote_identity_path().expect("remote_identity_path failed");
        let data = data_root().expect("data_root failed");

        assert!(identity.starts_with(&data));
        assert!(identity.ends_with("remote_identity"));
        // The parent must be `data/`, which is what `.gitignore` covers. A key
        // that landed a level up would be untracked rather than ignored.
        assert_eq!(
            identity.parent().and_then(|p| p.file_name()),
            Some(std::ffi::OsStr::new("data")),
            "the identity file must sit inside the ignored data directory"
        );
    }

    /// The keys a joining machine keeps sit under `data/`, which `.gitignore`
    /// covers, apart from the serving key, and naming the directory creates
    /// nothing. The data root is a directory this test made, so nothing was
    /// there before the call.
    #[test]
    fn the_join_keys_sit_under_data_apart_from_the_serving_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().expect("tempdir");
        let _env = EnvVarGuard::set("GGLIB_DATA_DIR", root.path().to_string_lossy().as_ref());

        let dir = remote_join_dir().expect("remote_join_dir");

        assert_eq!(dir, root.path().join("data").join("remote_join"));
        assert!(
            !root.path().join("data").exists(),
            "naming the directory made it, or made data/"
        );
        let serving = remote_identity_path().expect("remote_identity_path");
        assert!(
            !serving.starts_with(&dir),
            "the serving key is inside the join keys' directory"
        );
    }
}
