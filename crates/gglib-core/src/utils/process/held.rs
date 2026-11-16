//! Holding a downloaded program, and the folder it is in, so the file that
//! is checked is the file that runs.
//!
//! A program downloaded to be run is checked first, and then run by its
//! path. Between the two, another process of the same user could put
//! another file at that path, or another folder (a junction) under the
//! folder's name, and the one run would be one never checked. On Windows
//! the three calls here narrow that to nothing a file or folder operation
//! can do while the holds last:
//!
//! - [`hold_folder`] opens the folder itself, sharing it with no one who
//!   would delete or rename it, and refuses it if it is a reparse point (a
//!   junction or a link): its name keeps meaning this folder.
//! - [`create_private`] creates the file only if it is not there, shared
//!   for reading only while it is written, so it cannot be renamed aside or
//!   replaced during the download.
//! - [`hold_unchanged`] opens it again to read, shared for reading only, so
//!   nobody can write, delete or rename it until it is let go, and refuses
//!   a reparse point. Windows still starts the program from a file held
//!   this way.
//!
//! What is left between the download's handle closing and the hold being
//! taken is a moment in which the file could be swapped; the caller closes
//! it by comparing what the held file holds with what was downloaded. On
//! macOS and Linux there are no share modes: the folder is the caller's to
//! make so only this user can write in it, and a reparse point there is a
//! symbolic link, which is refused the same way.

use std::fs::File;
use std::io;
use std::path::Path;

/// Windows' `FILE_SHARE_READ`: others may open the file to read it (and so
/// to run it), and not to write, delete or rename it.
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 0x0000_0001;

/// Windows' `FILE_FLAG_BACKUP_SEMANTICS`: needed to open a folder.
#[cfg(windows)]
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

/// Windows' `FILE_FLAG_OPEN_REPARSE_POINT`: open a junction or a link
/// itself, not what it points at, so it can be seen and refused.
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

/// Windows' `FILE_ATTRIBUTE_REPARSE_POINT`.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Open the folder at `path` and hold it: while the answer is held nobody
/// else can rename or delete it on Windows, so its name keeps meaning this
/// folder.
///
/// # Errors
///
/// It cannot be opened, or it is a junction or a link, not a folder.
pub fn hold_folder(path: &Path) -> io::Result<File> {
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let held = options.open(path)?;
    refuse_a_link(path, &held)?;
    if !held.metadata()?.is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a folder",
            path.display()
        )));
    }
    Ok(held)
}

/// Create the file at `path` to write it, only if nothing is there, so
/// that while the answer is open nobody else can rename, delete or write it
/// on Windows.
///
/// # Errors
///
/// Something is there already, or it cannot be created.
pub fn create_private(path: &Path) -> io::Result<File> {
    let mut options = File::options();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.share_mode(FILE_SHARE_READ);
    }
    options.open(path)
}

/// Open the existing file at `path` to read it, so that while the answer is
/// held nobody else can change, delete or rename it on Windows.
///
/// # Errors
///
/// It cannot be opened (missing, or on Windows open for writing or deleting
/// by someone else), or it is a junction or a link.
pub fn hold_unchanged(path: &Path) -> io::Result<File> {
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let held = options.open(path)?;
    refuse_a_link(path, &held)?;
    Ok(held)
}

/// Refuse `held`, opened from `path`, if it is a reparse point (Windows) or
/// `path` is a symbolic link (macOS, Linux).
fn refuse_a_link(path: &Path, held: &File) -> io::Result<()> {
    #[cfg(windows)]
    let link = {
        use std::os::windows::fs::MetadataExt as _;
        held.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };
    #[cfg(not(windows))]
    let link = {
        let _ = held;
        std::fs::symlink_metadata(path)?.file_type().is_symlink()
    };
    if link {
        return Err(io::Error::other(format!(
            "{} is a link to somewhere else, so it was not used",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "held_tests.rs"]
mod tests;
