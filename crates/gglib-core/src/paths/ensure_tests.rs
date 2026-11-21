//! Whether a folder can be written, decided by writing to it.

use super::can_write_folder;

#[test]
fn a_folder_that_is_there_or_can_be_made_under_one_is_writable() {
    let root = tempfile::tempdir().expect("a temp folder");
    assert!(can_write_folder(root.path()));
    assert!(can_write_folder(&root.path().join("models").join("gguf")));
}

#[test]
fn a_path_under_a_file_is_not_a_folder_that_can_be_written() {
    let root = tempfile::tempdir().expect("a temp folder");
    let file = root.path().join("a-file");
    std::fs::write(&file, b"x").expect("a file");
    assert!(!can_write_folder(&file.join("models")));
}

/// A folder that refuses a write, and a missing folder under one: neither
/// can be written, whatever `readonly()` would say of the child.
#[test]
fn a_read_only_folder_and_a_missing_one_under_it_are_not_writable() {
    let root = tempfile::tempdir().expect("a temp folder");
    let locked = root.path().join("locked");
    std::fs::create_dir(&locked).expect("a folder to lock");
    let held = crate::paths::unwritable_folder(&locked);
    let folder = can_write_folder(&locked);
    let under = can_write_folder(&locked.join("models"));
    drop(held);
    assert!(!folder, "a folder that refuses a write");
    assert!(!under, "a missing folder under one");
}
