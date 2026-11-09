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

/// A folder whose mode forbids writing, and a missing folder under one:
/// neither can be written, whatever `readonly()` would say of the child.
#[cfg(unix)]
#[test]
fn a_read_only_folder_and_a_missing_one_under_it_are_not_writable() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().expect("a temp folder");
    let locked = root.path().join("locked");
    std::fs::create_dir(&locked).expect("a folder to lock");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("0555");
    let folder = can_write_folder(&locked);
    let under = can_write_folder(&locked.join("models"));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("writable again, to clean up");
    assert!(!folder, "a 0555 folder");
    assert!(!under, "a missing folder under a 0555 one");
}
