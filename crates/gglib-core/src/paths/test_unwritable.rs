//! A folder no file can be made in, made the same way on every OS.
//!
//! A test of what gglib does when a write is refused needs a folder that
//! refuses one. Taking the write bits off its mode does that on macOS and
//! Linux only, and a test that does so is compiled only there, so the code
//! it covers (which is the same on every OS) is never run on Windows.
//! [`unwritable_folder`] refuses the write on every OS.
//!
//! Compiled in this crate's own tests and with its `test-utils` feature,
//! which other crates enable from `[dev-dependencies]`.

use std::path::{Path, PathBuf};

/// A folder that refuses a write for as long as this is held. Dropping it
/// makes the folder writable again, so the temporary folder it is in can be
/// removed.
#[must_use = "the folder is writable again as soon as this is dropped"]
pub struct UnwritableFolder {
    folder: PathBuf,
    #[cfg(unix)]
    mode: u32,
}

/// Make the folder at `folder` refuse a write until the answer is dropped:
/// no file can be made in it. What is already in it can still be read, and
/// the folder can still be listed.
///
/// On macOS and Linux the write bits come off its mode, and go back on as
/// they were, and no folder can be made in it either. On Windows a deny
/// entry for everyone is added to its access list with `icacls`, and
/// removed. It asks for making a folder to be refused too, but Windows
/// still lets one be made, so a test that needs that refused stays
/// Unix-only.
///
/// A mode does not bind root, so a test that may run as root should check
/// that a write is refused before it asserts what follows from one.
///
/// # Panics
///
/// When `folder` is not there or cannot be changed, with the path and the
/// reason.
pub fn unwritable_folder(folder: impl AsRef<Path>) -> UnwritableFolder {
    let folder = folder.as_ref().to_path_buf();
    match refuse_writes(&folder) {
        Ok(held) => held,
        Err(why) => panic!(
            "unwritable_folder: could not make {} refuse a write: {why}",
            folder.display()
        ),
    }
}

impl Drop for UnwritableFolder {
    fn drop(&mut self) {
        // Best effort: a panic while a test is already unwinding would
        // abort the test binary, and a folder left unwritable only costs a
        // temporary folder that is not removed.
        let _ = self.allow_writes();
    }
}

#[cfg(unix)]
fn refuse_writes(folder: &Path) -> Result<UnwritableFolder, String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(folder)
        .map_err(|e| e.to_string())?
        .permissions()
        .mode();
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(mode & !0o222))
        .map_err(|e| e.to_string())?;
    Ok(UnwritableFolder {
        folder: folder.to_path_buf(),
        mode,
    })
}

#[cfg(unix)]
impl UnwritableFolder {
    fn allow_writes(&self) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&self.folder, std::fs::Permissions::from_mode(self.mode))
            .map_err(|e| e.to_string())
    }
}

/// Everyone, by its well-known identifier, so no account name is looked up.
#[cfg(windows)]
const EVERYONE: &str = "*S-1-1-0";

/// `WD` is "write data", which for a folder is making a file in it; `AD` is
/// "append data", which for a folder is making a folder in it. The `AD`
/// deny does not hold: CI's Windows runner still makes a folder there.
#[cfg(windows)]
fn refuse_writes(folder: &Path) -> Result<UnwritableFolder, String> {
    icacls(folder, &["/deny", &format!("{EVERYONE}:(WD,AD)")])?;
    Ok(UnwritableFolder {
        folder: folder.to_path_buf(),
    })
}

#[cfg(windows)]
impl UnwritableFolder {
    fn allow_writes(&self) -> Result<(), String> {
        icacls(&self.folder, &["/remove:d", EVERYONE])
    }
}

#[cfg(windows)]
fn icacls(folder: &Path, change: &[&str]) -> Result<(), String> {
    let ran = crate::utils::process::cmd("icacls")
        .arg(folder)
        .args(change)
        .output()
        .map_err(|e| format!("icacls did not start: {e}"))?;
    if ran.status.success() {
        Ok(())
    } else {
        Err(format!(
            "icacls {change:?} ended with {}: {} {}",
            ran.status,
            String::from_utf8_lossy(&ran.stdout).trim(),
            String::from_utf8_lossy(&ran.stderr).trim()
        ))
    }
}

#[cfg(test)]
#[path = "test_unwritable_tests.rs"]
mod tests;
