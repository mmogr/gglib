//! A held file can be read and run while it is held; on Windows nobody else
//! can write, delete or rename it until it is let go.

use std::io::Read as _;
use std::time::Duration;

use super::super::{bounded::run_bounded_silent, cmd};
use super::hold_unchanged;

/// The held file reads as what it is, and the program in it can be started
/// while it is held: the hold must not stop the run it protects.
#[test]
fn a_held_program_can_be_read_and_run() {
    let dir = tempfile::tempdir().unwrap();
    let program = gglib_test_proc::install(dir.path(), "installer", "exit 0");

    let mut held = hold_unchanged(&program).unwrap();
    let mut bytes = Vec::new();
    held.read_to_end(&mut bytes).unwrap();
    let status = run_bounded_silent(&mut cmd(&program), Duration::from_secs(30)).unwrap();

    assert!(!bytes.is_empty());
    assert!(status.success());
}

/// Nothing is created: there must be a file there to hold.
#[test]
fn nothing_is_held_where_there_is_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let refused = hold_unchanged(&dir.path().join("missing")).unwrap_err();
    assert_eq!(refused.kind(), std::io::ErrorKind::NotFound);
    assert!(!dir.path().join("missing").exists());
}

/// While it is held, another open to write it, its deletion and its
/// renaming are all refused; once let go, each works.
#[cfg(windows)]
#[test]
fn a_held_file_cannot_be_written_deleted_or_renamed_on_windows() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("installer.exe");
    std::fs::write(&file, b"checked").unwrap();
    let other = dir.path().join("other.exe");

    let held = hold_unchanged(&file).unwrap();
    assert!(std::fs::OpenOptions::new().write(true).open(&file).is_err());
    assert!(std::fs::remove_file(&file).is_err());
    assert!(std::fs::rename(&file, &other).is_err());
    assert!(
        std::fs::File::open(&file).is_ok(),
        "reading is still shared"
    );
    drop(held);

    assert!(std::fs::OpenOptions::new().write(true).open(&file).is_ok());
    std::fs::remove_file(&file).unwrap();
}
