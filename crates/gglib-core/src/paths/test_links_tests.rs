//! What [`link_folder`] promises a test, held on every OS: these run on
//! Windows too, where the link is a junction.

use std::fs;
use std::path::PathBuf;

use super::link_folder;
use crate::paths::canonical;

/// The names in `dir`, sorted.
fn names_in(dir: impl AsRef<std::path::Path>) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("the folder opens")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The link opens as the folder it points at: listing it, reading a file
/// through it and resolving it all reach the target.
#[test]
fn a_link_opens_as_the_folder_and_resolves_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("inside.txt"), b"in the real folder").unwrap();
    let link = dir.path().join("link");

    link_folder(&real, &link);

    assert!(link.is_dir());
    assert_eq!(names_in(&link), ["inside.txt"]);
    assert_eq!(
        fs::read(link.join("inside.txt")).unwrap(),
        b"in the real folder"
    );
    assert_eq!(canonical(&link).unwrap(), canonical(&real).unwrap());
    assert_eq!(
        canonical(link.join("inside.txt")).unwrap(),
        canonical(real.join("inside.txt")).unwrap()
    );
}

/// A link may point at the folder it is in, or at one above it, and two
/// links in one folder may both point at that folder.
#[test]
fn a_link_may_point_at_its_own_folder_or_one_above() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    fs::create_dir(&a).unwrap();
    fs::write(a.join("file.txt"), b"x").unwrap();

    link_folder(&a, a.join("l1"));
    link_folder(&a, a.join("l2"));
    link_folder(root.path(), a.join("up"));

    assert_eq!(names_in(a.join("l1")), ["file.txt", "l1", "l2", "up"]);
    assert_eq!(names_in(a.join("l2")), ["file.txt", "l1", "l2", "up"]);
    assert_eq!(names_in(a.join("up")), ["a"]);
    assert_eq!(canonical(a.join("l1")).unwrap(), canonical(&a).unwrap());
    assert_eq!(
        canonical(a.join("up")).unwrap(),
        canonical(root.path()).unwrap()
    );
}

/// A link to a folder that was never made is made all the same, and is a
/// link that leads nowhere.
#[test]
fn a_link_to_a_folder_that_is_not_there_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let nowhere = dir.path().join("never-made");
    let link = dir.path().join("link");

    link_folder(&nowhere, &link);

    assert!(
        fs::symlink_metadata(&link).is_ok(),
        "the link itself is there"
    );
    assert!(fs::read_dir(&link).is_err(), "and it opens as nothing");
    assert!(!nowhere.exists(), "making the link made no folder");
}

/// A link whose folder is removed afterwards stays, leading nowhere.
#[test]
fn a_link_outlives_the_folder_it_pointed_at() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    link_folder(&real, &link);

    fs::remove_dir(&real).unwrap();

    assert!(
        fs::symlink_metadata(&link).is_ok(),
        "the link itself is there"
    );
    assert!(fs::read_dir(&link).is_err(), "and it opens as nothing");
}

/// A temporary folder that holds links is removed with them, a link to its
/// own folder included, and the folder a link pointed at keeps its files.
#[test]
fn removing_a_folder_of_links_leaves_what_they_point_at() {
    let kept = tempfile::tempdir().unwrap();
    fs::write(kept.path().join("kept.txt"), b"still here").unwrap();
    let holder = tempfile::tempdir().unwrap();
    let held: PathBuf = holder.path().to_path_buf();
    link_folder(kept.path(), held.join("out"));
    link_folder(&held, held.join("self"));
    link_folder(held.join("never-made"), held.join("nowhere"));

    holder.close().expect("the folder of links is removed");

    assert!(!held.exists());
    assert_eq!(
        fs::read(kept.path().join("kept.txt")).unwrap(),
        b"still here"
    );
}

/// A link that cannot be made stops the test, saying both paths and why.
#[test]
#[should_panic(expected = "link_folder: could not make")]
fn a_link_that_cannot_be_made_panics_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let taken = dir.path().join("taken");
    fs::create_dir(&taken).unwrap();

    link_folder(dir.path(), &taken);
}
