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

/// The finished file's mode is not the whole story: every key passes through
/// the temporary file first, and one created `0644` and tightened after the
/// write is readable by anybody until the chmod, and for good if the process
/// dies in between. So the create step is checked on its own, before
/// `restrict` has had a chance to hide it.
///
/// It bites wherever the bug does. Under the usual `022` umask a plain create
/// is `0644`; under one that already hides group and other, such as `077`, a
/// plain create is `0600` too, there is no window, and this cannot tell the
/// two apart. The umask is not set here to force the question: it is
/// process-wide, and these tests run in parallel.
#[cfg(unix)]
#[test]
fn the_staging_file_is_unreadable_to_anybody_else_before_any_chmod() {
    use std::os::unix::fs::PermissionsExt;

    let path = temp();
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    create_private(&path).expect("create the staging file");

    let mode = std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o077,
        0,
        "group and other must have nothing from the first byte: {mode:o}"
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

    assert_eq!(staging_files(&path), 0, "the staging file is gone");
    assert_eq!(load(&path).expect("load").len(), 2);
}

/// Concurrent writers must not fail each other.
///
/// The staging file used to be one fixed `.tmp` sibling, so two writers wrote
/// the same name: the first renamed it away and the second's own `rename`
/// answered `NotFound` — a hard error for a write that was entirely valid.
/// Two `RemoteOps` in one process is all it takes, which is what a test
/// binary building an app per test does, and CI found it before a person did.
#[test]
fn writers_racing_on_one_path_do_not_fail_each_other() {
    let path = temp();
    let attempts = 16;

    let failures: Vec<String> = std::thread::scope(|scope| {
        // The `collect` is what makes this a race: it spawns every writer
        // before any is joined. Fused into the `filter_map` below, each
        // thread would be joined the moment it was spawned and the test
        // would pass against the very bug it exists to catch.
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = (0..attempts)
            .map(|i| {
                let path = path.clone();
                scope.spawn(move || {
                    let mut keys = DeviceKeys::new();
                    keys.insert(format!("dev-{i:08x}"), format!("sk-{i}"));
                    store(&path, &keys).err().map(|e| e.to_string())
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().expect("writer panicked"))
            .collect()
    });

    assert!(
        failures.is_empty(),
        "every writer's rename must find its own staging file: {failures:?}"
    );
    assert_eq!(
        load(&path).expect("load").len(),
        1,
        "and the file is one writer's whole map, never a blend of two"
    );
    assert_eq!(staging_files(&path), 0, "no staging file is left behind");
}

/// How many staging siblings sit beside `path`. Named per writer now, so the
/// count matters rather than one predictable name.
fn staging_files(path: &std::path::Path) -> usize {
    let dir = path.parent().expect("the key file has a directory");
    let stem = path.file_name().expect("the key file has a name");
    std::fs::read_dir(dir)
        .expect("read the key directory")
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&*stem.to_string_lossy()) && name.contains(".tmp")
        })
        .count()
}
