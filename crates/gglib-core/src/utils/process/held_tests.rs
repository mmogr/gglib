//! A held file can be read and run while it is held; a held folder can
//! have files made in it; a file is created only where nothing is; links
//! are refused. On Windows nobody else can write, delete or rename what is
//! held until it is let go.

use std::io::{Read as _, Write as _};
use std::time::Duration;

use super::super::{bounded::run_bounded_silent, cmd};
use super::{create_private, hold_folder, hold_unchanged};

/// The held file reads as what it is, and the program in it can be started
/// while it and its folder are held: the holds must not stop the run they
/// protect.
#[test]
fn a_held_program_can_be_read_and_run() {
    let dir = tempfile::tempdir().unwrap();
    let folder = hold_folder(dir.path()).unwrap();
    let program = gglib_test_proc::install(dir.path(), "installer", "exit 0");

    let mut held = hold_unchanged(&program).unwrap();
    let mut bytes = Vec::new();
    held.read_to_end(&mut bytes).unwrap();
    let status = run_bounded_silent(&mut cmd(&program), Duration::from_secs(30)).unwrap();

    assert!(!bytes.is_empty());
    assert!(status.success());
    drop(folder);
}

/// Nothing is held where there is no file.
#[test]
fn nothing_is_held_where_there_is_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let refused = hold_unchanged(&dir.path().join("missing")).unwrap_err();
    assert_eq!(refused.kind(), std::io::ErrorKind::NotFound);
    assert!(!dir.path().join("missing").exists());
}

/// A file is created only where nothing is: one already there is refused
/// and left as it was.
#[test]
fn a_file_is_created_only_where_nothing_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("installer.exe");
    let mut created = create_private(&path).unwrap();
    created.write_all(b"downloaded").unwrap();
    drop(created);

    let refused = create_private(&path).unwrap_err();

    assert_eq!(refused.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&path).unwrap(), b"downloaded");
}

/// A folder that is a link to another is refused, and so is a file that
/// is one: the name must mean the thing itself.
// Unix-only: it tests the symbolic-link branch `refuse_a_link` has off
// Windows. Its Windows twin is the junction test below.
#[cfg(unix)]
#[test]
fn a_link_is_refused_on_unix() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::write(real.join("installer"), b"x").unwrap();
    let linked = dir.path().join("linked");
    std::os::unix::fs::symlink(&real, &linked).unwrap();
    let file_link = dir.path().join("file-link");
    std::os::unix::fs::symlink(real.join("installer"), &file_link).unwrap();

    assert!(hold_folder(&real).is_ok());
    assert!(hold_folder(&linked).is_err());
    assert!(hold_unchanged(&file_link).is_err());
    assert!(
        hold_folder(&real.join("installer")).is_err(),
        "a file is not a folder"
    );
}

/// A junction in a folder's place is refused, and a folder held cannot be
/// renamed or deleted while files are still made in it.
#[cfg(windows)]
#[test]
fn a_held_folder_stays_put_and_a_junction_is_refused_on_windows() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("install");
    std::fs::create_dir(&folder).unwrap();

    let held = hold_folder(&folder).unwrap();
    std::fs::write(folder.join("made-inside"), b"x").unwrap();
    assert!(std::fs::rename(&folder, dir.path().join("moved")).is_err());
    drop(held);
    std::fs::remove_file(folder.join("made-inside")).unwrap();

    let junction = dir.path().join("junction");
    let made = super::super::cmd("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&folder)
        .status()
        .unwrap();
    assert!(made.success(), "mklink /J needs no privilege");
    assert!(hold_folder(&junction).is_err());
}

/// While it is held, another open to write it, its deletion and its
/// renaming are all refused; once let go, each works. A file being written
/// through `create_private` cannot be renamed aside either.
#[cfg(windows)]
#[test]
fn a_held_file_cannot_be_written_deleted_or_renamed_on_windows() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("installer.exe");
    let other = dir.path().join("other.exe");

    let writing = create_private(&file).unwrap();
    assert!(std::fs::rename(&file, &other).is_err());
    assert!(std::fs::remove_file(&file).is_err());
    drop(writing);

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
