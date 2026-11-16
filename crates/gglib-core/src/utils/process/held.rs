//! Holding a program's file so it stays the file it is while it is checked
//! and run.
//!
//! A program downloaded to be run is checked first, and then run by its
//! path. Between the two, another process could put another file at that
//! path, and the one run would be one never checked. [`hold_unchanged`]
//! opens the file so that, for as long as it is held, nobody else can
//! write it, delete it or rename it on Windows: it shares the file for
//! reading only, which still lets Windows start the program from it. The
//! path checked and the path run are then the one held file.
//!
//! macOS and Linux have no such share mode: a file can always be replaced
//! by its owner there. The folder it sits in is the caller's to make so
//! that only this user can write in it.

use std::fs::File;
use std::io;
use std::path::Path;

/// Windows' `FILE_SHARE_READ`: others may open the file to read it (and so
/// to run it), and not to write, delete or rename it.
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 0x0000_0001;

/// Open the existing file at `path` to read it, so that while the answer is
/// held nobody else can change, delete or rename it on Windows.
///
/// # Errors
///
/// It cannot be opened: missing, or (on Windows) already open for writing
/// or deleting by someone else.
pub fn hold_unchanged(path: &Path) -> io::Result<File> {
    let mut options = File::options();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.share_mode(FILE_SHARE_READ);
    }
    options.open(path)
}

#[cfg(test)]
#[path = "held_tests.rs"]
mod tests;
