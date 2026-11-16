//! Tests for [`super`]: the text the Windows installer leaves in the user's
//! `PATH` when it puts its folder there and takes it off again. Every case
//! runs on every OS.

use super::*;

const FOLDER: &str = r"C:\Users\matt\AppData\Local\gglib-gui";

// ── Adding ────────────────────────────────────────────────────────────────

#[test]
fn an_empty_path_becomes_the_folder() {
    assert_eq!(with_folder("", FOLDER).as_deref(), Some(FOLDER));
}

#[test]
fn the_folder_is_added_after_the_last_entry() {
    assert_eq!(
        with_folder(r"C:\tools", FOLDER),
        Some(format!(r"C:\tools;{FOLDER}"))
    );
}

#[test]
fn a_path_ending_in_a_separator_keeps_its_ending() {
    assert_eq!(
        with_folder(r"C:\tools;", FOLDER),
        Some(format!(r"C:\tools;{FOLDER};"))
    );
}

#[test]
fn a_folder_already_there_is_not_added_again() {
    let path = format!(r"C:\tools;{FOLDER};D:\bin");
    assert_eq!(with_folder(&path, FOLDER), None);
}

#[test]
fn a_folder_there_in_another_case_is_not_added_again() {
    let path = r"C:\tools;c:\users\MATT\appdata\local\GGLIB-GUI";
    assert_eq!(with_folder(path, FOLDER), None);
}

#[test]
fn a_folder_there_with_a_trailing_backslash_is_not_added_again() {
    let path = format!(r"C:\tools;{FOLDER}\");
    assert_eq!(with_folder(&path, FOLDER), None);
}

#[test]
fn a_folder_there_in_quotes_is_not_added_again() {
    let path = format!(r#"C:\tools;"{FOLDER}""#);
    assert_eq!(with_folder(&path, FOLDER), None);
}

#[test]
fn a_folder_there_with_a_non_ascii_name_in_another_case_is_not_added_again() {
    let path = r"C:\Users\ÖLAF\gglib-gui";
    assert_eq!(with_folder(path, r"C:\Users\ölaf\gglib-gui"), None);
}

#[test]
fn quoted_and_variable_entries_are_kept_as_written() {
    let path = r#""C:\Program Files\Tool";%USERPROFILE%\bin;%LOCALAPPDATA%\Microsoft\WindowsApps"#;
    assert_eq!(with_folder(path, FOLDER), Some(format!("{path};{FOLDER}")));
}

#[test]
fn empty_entries_are_kept() {
    assert_eq!(
        with_folder(r"C:\a;;C:\b", FOLDER),
        Some(format!(r"C:\a;;C:\b;{FOLDER}"))
    );
}

#[test]
fn a_longer_folder_with_the_same_start_is_another_folder() {
    let path = format!(r"{FOLDER}-old");
    assert_eq!(with_folder(&path, FOLDER), Some(format!("{path};{FOLDER}")));
}

#[test]
fn an_entry_naming_the_folder_through_a_variable_is_another_entry() {
    // The text is compared, not what it expands to: `%LOCALAPPDATA%` is
    // whatever the variable holds when a program starts.
    let path = r"%LOCALAPPDATA%\gglib-gui";
    assert_eq!(with_folder(path, FOLDER), Some(format!("{path};{FOLDER}")));
}

#[test]
fn a_path_longer_than_an_nsis_string_is_kept_whole() {
    // The stock NSIS build holds 1024 characters in a string; a PATH read
    // into one and written back would lose the rest.
    let path = (0..200)
        .map(|n| format!(r"C:\tools\number-{n:04}"))
        .collect::<Vec<_>>()
        .join(";");
    assert!(path.len() > 4096);
    assert_eq!(with_folder(&path, FOLDER), Some(format!("{path};{FOLDER}")));
}

// ── Removing ──────────────────────────────────────────────────────────────

#[test]
fn the_folder_is_taken_out_from_the_middle() {
    let path = format!(r"C:\tools;{FOLDER};D:\bin");
    assert_eq!(
        without_folder(&path, FOLDER).as_deref(),
        Some(r"C:\tools;D:\bin")
    );
}

#[test]
fn the_folder_is_taken_out_from_the_end() {
    let path = format!(r"C:\tools;{FOLDER}");
    assert_eq!(without_folder(&path, FOLDER).as_deref(), Some(r"C:\tools"));
}

#[test]
fn the_folder_is_taken_out_from_the_start() {
    let path = format!(r"{FOLDER};C:\tools");
    assert_eq!(without_folder(&path, FOLDER).as_deref(), Some(r"C:\tools"));
}

#[test]
fn a_path_of_the_folder_alone_becomes_empty() {
    assert_eq!(without_folder(FOLDER, FOLDER).as_deref(), Some(""));
}

#[test]
fn the_folder_there_twice_is_taken_out_twice() {
    let path = format!(r"{FOLDER};C:\tools;{FOLDER}\");
    assert_eq!(without_folder(&path, FOLDER).as_deref(), Some(r"C:\tools"));
}

#[test]
fn a_path_without_the_folder_is_left_alone() {
    assert_eq!(without_folder(r"C:\tools;D:\bin", FOLDER), None);
    assert_eq!(without_folder("", FOLDER), None);
}

#[test]
fn a_longer_folder_with_the_same_start_is_not_taken_out() {
    let old = format!(r"{FOLDER}-old");
    assert_eq!(without_folder(&old, FOLDER), None);
    let path = format!(r"{old};{FOLDER}");
    assert_eq!(without_folder(&path, FOLDER), Some(old));
}

#[test]
fn every_other_entry_is_kept_as_written() {
    let path = format!(r#""C:\Program Files\Tool";;{FOLDER};%USERPROFILE%\bin;"#);
    assert_eq!(
        without_folder(&path, FOLDER).as_deref(),
        Some(r#""C:\Program Files\Tool";;%USERPROFILE%\bin;"#)
    );
}

// ── Adding, then removing ─────────────────────────────────────────────────

#[test]
fn removing_what_was_added_gives_back_the_path_as_it_was() {
    let long = (0..200)
        .map(|n| format!(r"C:\tools\number-{n:04}"))
        .collect::<Vec<_>>()
        .join(";");
    for before in [
        "",
        r"C:\tools",
        r"C:\tools;",
        r"C:\a;;C:\b;",
        r#""C:\Program Files\Tool";%USERPROFILE%\bin"#,
        long.as_str(),
    ] {
        let added = with_folder(before, FOLDER).expect("the folder is not there yet");
        assert_eq!(
            with_folder(&added, FOLDER),
            None,
            "added twice to {before:?}"
        );
        assert_eq!(
            without_folder(&added, FOLDER).as_deref(),
            Some(before),
            "round trip of {before:?}"
        );
    }
}
