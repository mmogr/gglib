//! Opening a library, and the reading it holds for its verdicts.

use tempfile::TempDir;

use super::*;
use crate::test_hub::TestHub;
use crate::test_library::{PHONE, dirs_in, library_over, wait};

#[test]
fn a_library_opens_in_folders_it_makes_and_holds_nothing_yet() {
    let root = TempDir::new().unwrap();
    let dirs = dirs_in(&root);

    let library = wait(open_library(dirs.clone(), None)).expect("the library opens");

    assert!(
        std::path::Path::new(&dirs.data_dir)
            .join(DATABASE_FILE)
            .is_file()
    );
    assert!(std::path::Path::new(&dirs.models_dir).is_dir());
    assert_eq!(wait(library.installed()), Ok(Vec::new()));
}

/// A second open of the same folders finds what the first left.
#[test]
fn a_library_opened_again_is_the_same_library() {
    let root = TempDir::new().unwrap();
    let first = wait(open_library(dirs_in(&root), None)).unwrap();
    drop(first);

    let again = wait(open_library(
        dirs_in(&root),
        Some("hf_not_a_real_token".to_owned()),
    ));

    assert!(again.is_ok());
}

#[test]
fn a_library_cannot_be_opened_where_a_folder_cannot_be_made() {
    let root = TempDir::new().unwrap();
    let file = root.path().join("a-file");
    std::fs::write(&file, b"not a folder").unwrap();
    let dirs = LibraryDirs {
        data_dir: file.join("data").to_string_lossy().into_owned(),
        models_dir: root.path().join("models").to_string_lossy().into_owned(),
    };

    let opened = wait(open_library(dirs, None));

    assert!(
        matches!(opened, Err(GglibError::Internal { .. })),
        "{:?}",
        opened.err()
    );
}

/// The reading handed to a browse is held, and the installed list is
/// judged at it; before one, nothing has a budget.
#[test]
fn installed_models_are_judged_at_the_reading_last_handed_over() {
    let root = TempDir::new().unwrap();
    let library = library_over(TestHub::default(), &root);

    assert_eq!(library.held_budget(), None);
    let _ = library.suggested(PHONE);

    assert_eq!(library.held_budget(), PHONE.budget());
}
