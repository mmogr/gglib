//! What [`unwritable_folder`] promises a test, held on every OS.

use std::fs;

use super::unwritable_folder;

/// While it is held no file can be made in the folder, what was there can
/// still be read, and the folder can be listed; once it is dropped a write
/// works again and the temporary folder is removed.
#[test]
fn a_held_folder_refuses_a_write_and_a_dropped_one_takes_it() {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("locked");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("already-there"), b"kept").unwrap();

    let held = unwritable_folder(&folder);
    let file = fs::write(folder.join("a-file"), b"x");
    // Unix only: a folder cannot be made in it either; Windows lets one be made.
    #[cfg(unix)]
    {
        let sub = fs::create_dir(folder.join("a-folder"));
        assert!(sub.is_err(), "a folder was made in it");
    }
    let read = fs::read(folder.join("already-there"));
    let listed = fs::read_dir(&folder).map(Iterator::count);
    drop(held);
    let after = fs::write(folder.join("a-file"), b"x");

    after.expect("a write works once the folder is let go");
    root.close().expect("and the folder can be removed");
    assert!(file.is_err(), "a file was made in it");
    assert_eq!(read.expect("what was there is read"), b"kept");
    assert_eq!(listed.expect("the folder is listed"), 1);
}

/// A folder that is not there stops the test, saying which and why.
#[test]
#[should_panic(expected = "unwritable_folder: could not make")]
fn a_folder_that_is_not_there_panics_with_the_reason() {
    let root = tempfile::tempdir().unwrap();

    let _held = unwritable_folder(root.path().join("never-made"));
}
