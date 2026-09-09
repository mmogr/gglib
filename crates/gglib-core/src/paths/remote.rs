//! Where the remote tunnel's stored endpoint key lives.
//!
//! Only `gglib remote enable --keep-identity` writes here, and only when it is
//! asked to; without the flag the tunnel mints a fresh endpoint key per session
//! and nothing is left on disk (ADR 0012, decision 4).

use std::fs;
use std::path::PathBuf;

use super::error::PathError;
use super::platform::data_root;

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

    fs::create_dir_all(&data_dir).map_err(|e| PathError::CreateFailed {
        path: data_dir.clone(),
        reason: e.to_string(),
    })?;

    Ok(data_dir.join("remote_identity"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_utils::ENV_LOCK;

    #[test]
    fn remote_identity_is_under_the_ignored_data_directory() {
        let _guard = ENV_LOCK.lock().unwrap();
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
}
