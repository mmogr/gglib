//! A folder link a test makes the same way on every OS.
//!
//! A test of code that resolves a path, or refuses a link, needs a folder
//! reached under a second name. `std::os::unix::fs::symlink` makes one on
//! macOS and Linux only, and a test that calls it is compiled only there, so
//! the code it covers is never run on Windows. [`link_folder`] makes the
//! link on every OS, and a test that uses it needs no gate.
//!
//! Compiled in this crate's own tests and with its `test-utils` feature,
//! which other crates enable from `[dev-dependencies]`.

use std::path::Path;

/// Make `link` a link to the folder `target`: a symbolic link on macOS and
/// Linux, a junction on Windows. The arguments are in the order
/// `std::os::unix::fs::symlink` takes them, the target first.
///
/// What a test can rely on, on every OS:
///
/// - **`target` is an absolute path.** A junction holds an absolute path, so
///   a relative one is not given the same meaning everywhere.
/// - **`target` may be any folder**, the folder `link` is made in and the
///   folders above that one included. A link can point at its own parent.
/// - **`target` does not have to be there.** Nothing here reads it, when the
///   link is made or after, so a test can link to a folder it has not made,
///   or remove the folder afterwards, and hold a link that leads nowhere.
/// - **`link` must not be there**, and the folder it is made in must be.
/// - **`link` opens as the folder**: `read_dir`, a path through it and
///   [`super::canonical`] all reach `target`.
/// - **Removing the link's folder does not reach `target`.** A temporary
///   folder that holds links is removed with them, and what they point at
///   is left as it was.
///
/// A link to a file is not made here: Windows lets an ordinary account make
/// one only in Developer Mode, so a test that needs one stays Unix-only.
///
/// # Panics
///
/// When the link cannot be made, with both paths and the reason, so a test
/// needs no error handling of its own.
pub fn link_folder(target: impl AsRef<Path>, link: impl AsRef<Path>) {
    let (target, link) = (target.as_ref(), link.as_ref());
    if let Err(why) = make(target, link) {
        panic!(
            "link_folder: could not make {} a link to {}: {why}",
            link.display(),
            target.display()
        );
    }
}

#[cfg(unix)]
fn make(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| e.to_string())
}

/// `mklink /J`, which needs no privilege. It is a command of `cmd.exe` and
/// not a program, so it is run through one.
#[cfg(windows)]
fn make(target: &Path, link: &Path) -> Result<(), String> {
    let ran = crate::utils::process::cmd("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .map_err(|e| format!("cmd.exe did not start: {e}"))?;
    if ran.status.success() {
        Ok(())
    } else {
        Err(format!(
            "mklink /J ended with {}: {}",
            ran.status,
            String::from_utf8_lossy(&ran.stderr).trim()
        ))
    }
}

#[cfg(test)]
#[path = "test_links_tests.rs"]
mod tests;
