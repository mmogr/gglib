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

// ── The folder ────────────────────────────────────────────────────────────

#[test]
fn a_folder_that_cannot_be_one_entry_is_refused_before_anything_is_read() {
    for folder in [r"C:\a;b", r#"C:\a"b"#, ""] {
        let error = add(Path::new(folder)).expect_err(folder);
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{folder:?}");
        let error = remove(Path::new(folder)).expect_err(folder);
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{folder:?}");
    }
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
        r#""D:\x;C:\y";E:\z"#,
        r#"C:\a;"D:\x;E:\y"#,
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

// ── Quoted entries ────────────────────────────────────────────────────────

#[test]
fn a_separator_inside_quotes_is_part_of_the_entry() {
    let quoted = format!(r#""D:\x;{FOLDER}""#);
    let path = format!(r"{quoted};E:\y");
    assert_eq!(entries(&path), [quoted.as_str(), r"E:\y"]);
    assert_eq!(without_folder(&path, FOLDER), None);
    assert_eq!(with_folder(&path, FOLDER), Some(format!("{path};{FOLDER}")));
}

#[test]
fn the_folder_between_two_quoted_separators_is_not_cut_out() {
    let path = format!(r#""D:\x;{FOLDER};E:\y";F:\z"#);
    assert_eq!(without_folder(&path, FOLDER), None);
    assert_eq!(with_folder(&path, FOLDER), Some(format!("{path};{FOLDER}")));
}

#[test]
fn an_unclosed_quote_makes_the_rest_one_entry() {
    let open = format!(r#""D:\x;{FOLDER};E:\y"#);
    let path = format!(r"C:\a;{open}");
    assert_eq!(entries(&path), [r"C:\a", open.as_str()]);
    assert_eq!(without_folder(&path, FOLDER), None);
}

#[test]
fn a_path_ending_inside_an_unclosed_quote_gets_the_folder_first() {
    let path = r#"C:\a;"D:\x"#;
    let added = with_folder(path, FOLDER).expect("not there yet");
    assert_eq!(added, format!("{FOLDER};{path}"));
    assert_eq!(with_folder(&added, FOLDER), None);
    assert_eq!(without_folder(&added, FOLDER).as_deref(), Some(path));
}

#[test]
fn a_quoted_folder_beside_quoted_separators_is_taken_out_whole() {
    let path = format!(r#""D:\x;E:\y";"{FOLDER}";F:\z"#);
    assert_eq!(
        without_folder(&path, FOLDER).as_deref(),
        Some(r#""D:\x;E:\y";F:\z"#)
    );
}

// ── The order of reads and writes, against a stand-in ─────────────────────

/// A stand-in for the registry: the user's `PATH`, the record, every write
/// in order, and reads or writes made to fail.
#[derive(Default)]
struct Fake {
    path: Option<(String, bool)>,
    not_kept: bool,
    record: String,
    writes: Vec<String>,
    fails: Option<Fail>,
}

/// The one call a [`Fake`] makes fail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fail {
    PathRead,
    RecordRead,
    PathWrite,
}

impl Fake {
    fn with_path(text: &str, expandable: bool) -> Self {
        Self {
            path: Some((text.to_owned(), expandable)),
            ..Self::default()
        }
    }

    fn path_text(&self) -> &str {
        self.path.as_ref().map_or("", |(text, _)| text)
    }
}

fn refused() -> io::Error {
    io::Error::other("refused")
}

impl Store for Fake {
    fn path(&mut self) -> io::Result<UserPath> {
        if self.fails == Some(Fail::PathRead) {
            return Err(refused());
        }
        Ok(match (&self.path, self.not_kept) {
            (_, true) => UserPath::NotKept,
            (None, false) => UserPath::Absent,
            (Some((text, expandable)), false) => UserPath::Text {
                text: text.clone(),
                expandable: *expandable,
            },
        })
    }

    fn set_path(&mut self, text: &str, expandable: bool) -> io::Result<()> {
        if self.fails == Some(Fail::PathWrite) {
            return Err(refused());
        }
        self.writes.push(format!("path {text} {expandable}"));
        self.path = Some((text.to_owned(), expandable));
        Ok(())
    }

    fn record(&mut self) -> io::Result<String> {
        if self.fails == Some(Fail::RecordRead) {
            return Err(refused());
        }
        Ok(self.record.clone())
    }

    fn set_record(&mut self, text: &str) -> io::Result<()> {
        self.writes.push(format!("record {text}"));
        self.record = text.to_owned();
        Ok(())
    }
}

fn folder() -> &'static Path {
    Path::new(FOLDER)
}

#[test]
fn a_missing_path_is_made_expandable_after_the_record() {
    let mut store = Fake::default();
    assert_eq!(add_in(&mut store, folder()).unwrap(), Edit::Changed);
    assert_eq!(
        store.writes,
        [format!("record {FOLDER}"), format!("path {FOLDER} true")]
    );
}

#[test]
fn the_type_found_is_the_type_written() {
    let mut store = Fake::with_path(r"C:\tools", false);
    add_in(&mut store, folder()).unwrap();
    assert_eq!(store.path, Some((format!(r"C:\tools;{FOLDER}"), false)));
    remove_in(&mut store, folder()).unwrap();
    assert_eq!(store.path, Some((r"C:\tools".to_owned(), false)));
}

#[test]
fn a_failed_read_writes_nothing() {
    // A value of a refused type, REG_MULTI_SZ or REG_BINARY, is a failed
    // read: the registry refuses it before gglib sees it.
    for fails in [Fail::PathRead, Fail::RecordRead] {
        let mut store = Fake::with_path(r"C:\tools", true);
        store.record = FOLDER.to_owned();
        store.fails = Some(fails);
        add_in(&mut store, folder()).expect_err("add");
        remove_in(&mut store, folder()).expect_err("remove");
        assert!(store.writes.is_empty(), "{fails:?}: {:?}", store.writes);
    }
}

#[test]
fn a_failed_path_write_puts_the_record_back() {
    let mut store = Fake::with_path(r"C:\tools", true);
    store.record = r"D:\elsewhere".to_owned();
    store.fails = Some(Fail::PathWrite);
    add_in(&mut store, folder()).expect_err("add");
    assert_eq!(store.record, r"D:\elsewhere");
    assert_eq!(store.path_text(), r"C:\tools");
}

#[test]
fn a_folder_on_path_before_the_install_is_left_there() {
    let before = format!(r"C:\tools;{FOLDER}");
    let mut store = Fake::with_path(&before, true);
    assert_eq!(add_in(&mut store, folder()).unwrap(), Edit::Unchanged);
    assert_eq!(remove_in(&mut store, folder()).unwrap(), Edit::NotOurs);
    assert!(store.writes.is_empty(), "{:?}", store.writes);
    assert_eq!(store.path_text(), before);
}

#[test]
fn what_the_install_added_the_uninstall_takes_off_with_its_record() {
    let mut store = Fake::with_path(r"C:\tools", true);
    add_in(&mut store, folder()).unwrap();
    store.writes.clear();
    assert_eq!(remove_in(&mut store, folder()).unwrap(), Edit::Changed);
    assert_eq!(store.writes, [r"path C:\tools true", "record "]);
    assert_eq!(store.path_text(), r"C:\tools");
    assert_eq!(store.record, "");
}

#[test]
fn installing_twice_keeps_one_entry_and_the_record() {
    let mut store = Fake::with_path(r"C:\tools", true);
    add_in(&mut store, folder()).unwrap();
    store.writes.clear();
    assert_eq!(add_in(&mut store, folder()).unwrap(), Edit::Unchanged);
    assert!(store.writes.is_empty(), "{:?}", store.writes);
    assert_eq!(store.path_text(), format!(r"C:\tools;{FOLDER}"));
    assert_eq!(store.record, FOLDER);
}

#[test]
fn an_entry_the_user_took_off_leaves_path_alone_and_clears_the_record() {
    let mut store = Fake::with_path(r"C:\tools", true);
    store.record = FOLDER.to_owned();
    assert_eq!(remove_in(&mut store, folder()).unwrap(), Edit::Unchanged);
    assert_eq!(store.writes, ["record "]);
    assert_eq!(store.path_text(), r"C:\tools");
}

#[test]
fn a_record_of_another_folder_takes_nothing_of_this_one_off() {
    let other = r"D:\Apps\gglib-gui";
    let mut store = Fake::with_path(&format!(r"C:\tools;{FOLDER};{other}"), true);
    store.record = other.to_owned();
    assert_eq!(remove_in(&mut store, folder()).unwrap(), Edit::NotOurs);
    assert!(store.writes.is_empty(), "{:?}", store.writes);
}

#[test]
fn an_install_moved_keeps_both_folders_recorded() {
    let other = r"D:\Apps\gglib-gui";
    let mut store = Fake::with_path(r"C:\tools", true);
    add_in(&mut store, folder()).unwrap();
    add_in(&mut store, Path::new(other)).unwrap();
    assert_eq!(store.record, format!("{FOLDER};{other}"));
    assert_eq!(
        remove_in(&mut store, Path::new(other)).unwrap(),
        Edit::Changed
    );
    assert_eq!(store.path_text(), format!(r"C:\tools;{FOLDER}"));
    assert_eq!(store.record, FOLDER);
}

#[test]
fn a_path_left_empty_is_stored_empty() {
    let mut store = Fake::default();
    add_in(&mut store, folder()).unwrap();
    remove_in(&mut store, folder()).unwrap();
    assert_eq!(store.path, Some((String::new(), true)));
}

#[test]
fn an_os_without_a_user_path_writes_nothing() {
    let mut store = Fake {
        not_kept: true,
        record: FOLDER.to_owned(),
        ..Fake::default()
    };
    assert_eq!(add_in(&mut store, folder()).unwrap(), Edit::NotKept);
    assert_eq!(remove_in(&mut store, folder()).unwrap(), Edit::NotKept);
    assert!(store.writes.is_empty(), "{:?}", store.writes);
}
