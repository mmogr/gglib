//! The two rules a chat's folder is made by: where `gglib chat` was
//! launched, never home or a system folder, and what a person chose, once
//! it is an absolute path to a folder. Which folders are system folders is
//! `places_tests`'.

use std::path::{Path, PathBuf};

use super::*;
use crate::host::HostOs;

fn canonical(path: &Path) -> PathBuf {
    crate::paths::canonical(path).expect("a path that resolves")
}

/// The folder of a chat launched in `cwd` on this OS with `home` as its
/// home folder and no Windows folders named; `None` where none is adopted.
fn launched_in(cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let places = Places {
        os: HostOs::current(),
        home: home.map(Path::to_path_buf),
        windows_dir: None,
        program_files: Vec::new(),
    };
    launched_on(cwd, &places.resolved()).ok()
}

#[test]
fn a_chat_launched_in_a_folder_inside_home_reads_that_folder() {
    let home = tempfile::tempdir().unwrap();
    let project = home.path().join("project");
    std::fs::create_dir(&project).unwrap();

    assert_eq!(
        launched_in(&project, Some(home.path())),
        Some(canonical(&project))
    );
}

#[test]
fn a_chat_launched_in_home_reads_nothing_and_says_it_is_home() {
    let home = tempfile::tempdir().unwrap();
    let places = Places {
        os: HostOs::current(),
        home: Some(home.path().to_path_buf()),
        windows_dir: None,
        program_files: Vec::new(),
    }
    .resolved();

    assert_eq!(launched_in(home.path(), Some(home.path())), None);
    assert_eq!(launched_on(home.path(), &places), Err(NotAdopted::Home));
}

/// A system folder is refused and named canonical: a tempdir stands in for
/// the Windows directory, which is a value, so this runs on every OS.
#[test]
fn a_chat_launched_in_a_system_folder_reads_nothing_and_names_it() {
    let windows = tempfile::tempdir().unwrap();
    let system32 = windows.path().join("System32");
    std::fs::create_dir(&system32).unwrap();
    let places = Places {
        os: HostOs::Windows,
        home: None,
        windows_dir: Some(windows.path().to_path_buf()),
        program_files: Vec::new(),
    }
    .resolved();

    assert_eq!(
        launched_on(&system32, &places),
        Err(NotAdopted::System(canonical(&system32)))
    );
}

#[test]
fn home_is_recognised_by_where_it_resolves_not_how_it_is_spelled() {
    let home = tempfile::tempdir().unwrap();
    let spelled = home.path().join("project").join("..");
    std::fs::create_dir(home.path().join("project")).unwrap();

    assert_eq!(launched_in(&spelled, Some(home.path())), None);
    assert_eq!(launched_in(home.path(), Some(&spelled)), None);
}

#[cfg(unix)]
#[test]
fn a_home_reached_through_a_link_is_still_home() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    assert_eq!(launched_in(&real, Some(&link)), None);
}

#[test]
fn a_chat_with_no_home_reads_where_it_was_launched() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(launched_in(dir.path(), None), Some(canonical(dir.path())));
}

#[test]
fn a_chosen_folder_is_its_canonical_path() {
    let dir = tempfile::tempdir().unwrap();
    let inner = dir.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let spelled = inner.join("..").join("inner");

    assert_eq!(chosen(&spelled), Ok(canonical(&inner)));
}

#[test]
fn home_can_be_chosen() {
    let home = tempfile::tempdir().unwrap();

    assert_eq!(chosen(home.path()), Ok(canonical(home.path())));
}

#[test]
fn a_relative_path_is_not_a_chosen_folder() {
    for path in ["src", "", "./src"] {
        assert_eq!(
            chosen(Path::new(path)),
            Err(FolderError::NotAbsolute(PathBuf::from(path)))
        );
    }
}

#[test]
fn a_path_with_nothing_there_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("gone");

    assert_eq!(chosen(&gone), Err(FolderError::Missing(gone)));
}

#[test]
fn a_file_is_not_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();

    assert_eq!(chosen(&file), Err(FolderError::NotAFolder(file)));
}

#[test]
fn each_refusal_names_the_path_it_was_given() {
    let shown = |e: FolderError| e.to_string();

    assert_eq!(
        shown(FolderError::NotAbsolute("src".into())),
        "'src' is not an absolute path"
    );
    assert_eq!(
        shown(FolderError::Missing("/x/gone".into())),
        "no folder at '/x/gone'"
    );
    assert_eq!(
        shown(FolderError::NotAFolder("/x/notes.txt".into())),
        "'/x/notes.txt' is not a folder"
    );
}
