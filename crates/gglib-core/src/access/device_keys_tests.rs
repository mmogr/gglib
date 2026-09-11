//! Tests for the device key store.
//!
//! A `#[path]` sibling because the store and its tests together would cross
//! the 300-line budget, the split `bearer_tests.rs` already makes.

use super::*;

fn temp() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("gglib-device-keys-{}", uuid::Uuid::new_v4()));
    p.push("remote_devices");
    p
}

/// A machine that has never invited anything is not a machine in a bad
/// state, so the absent file reads as "nobody", not as a failure that would
/// stop `enable` arming.
#[test]
fn a_missing_file_is_an_empty_roster_rather_than_an_error() {
    let keys = load(&temp()).expect("a missing file is not an error");
    assert!(keys.is_empty());
}

#[test]
fn what_is_stored_comes_back() {
    let path = temp();
    let mut keys = DeviceKeys::new();
    keys.insert("dev-0a1b2c3d".to_owned(), "sk-one".to_owned());
    keys.insert("dev-4e5f6a7b".to_owned(), "sk-two".to_owned());

    store(&path, &keys).expect("store");
    assert_eq!(load(&path).expect("load"), keys);
}

/// Softening a parse failure into an empty roster would arm a listener
/// admitting nobody while the roster in settings still lists devices — the
/// operator would see them listed and refused at the same time, with nothing
/// saying why.
#[test]
fn a_corrupt_file_is_an_error_and_not_an_empty_roster() {
    let path = temp();
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, b"{ not json").expect("write");

    assert!(
        load(&path).is_err(),
        "a corrupt roster must not read as empty"
    );
}

/// The keys are secrets and the file sits in a directory a debug build
/// resolves to the repository checkout, so the mode is the whole protection.
#[cfg(unix)]
#[test]
fn the_file_is_written_unreadable_to_anybody_else() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp();
    let mut keys = DeviceKeys::new();
    keys.insert("dev-0a1b2c3d".to_owned(), "sk-secret".to_owned());
    store(&path, &keys).expect("store");

    let mode = std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o077,
        0,
        "group and other must have nothing: {mode:o}"
    );
}

/// A crash mid-write must leave the previous roster rather than a truncated
/// one, so the write goes to a sibling and renames.
#[test]
fn a_rewrite_leaves_no_temporary_behind() {
    let path = temp();
    let mut keys = DeviceKeys::new();
    keys.insert("dev-0a1b2c3d".to_owned(), "sk-one".to_owned());
    store(&path, &keys).expect("first");
    keys.insert("dev-4e5f6a7b".to_owned(), "sk-two".to_owned());
    store(&path, &keys).expect("second");

    assert!(
        !path.with_extension("tmp").exists(),
        "the staging file is gone"
    );
    assert_eq!(load(&path).expect("load").len(), 2);
}
