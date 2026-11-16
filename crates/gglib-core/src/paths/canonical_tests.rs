//! Tests for [`super`]. The rule is a function of a path's text, so every
//! case runs on every OS; the one test of a real canonical path proves the
//! stripping only where `canonicalize` writes the prefix, on Windows.

use super::*;

#[test]
fn a_verbatim_drive_path_loses_its_prefix() {
    assert_eq!(plain_form(r"\\?\C:\a\b").as_deref(), Some(r"C:\a\b"));
    assert_eq!(plain_form(r"\\?\d:\models").as_deref(), Some(r"d:\models"));
}

#[test]
fn a_verbatim_drive_root_loses_its_prefix() {
    assert_eq!(plain_form(r"\\?\C:\").as_deref(), Some(r"C:\"));
}

#[test]
fn a_verbatim_share_path_is_written_as_a_share() {
    assert_eq!(
        plain_form(r"\\?\UNC\server\share\x").as_deref(),
        Some(r"\\server\share\x")
    );
    assert_eq!(
        plain_form(r"\\?\UNC\server\share").as_deref(),
        Some(r"\\server\share")
    );
}

#[test]
fn a_share_root_loses_its_prefix_and_keeps_its_separator() {
    // As `canonicalize` gives a share's root, or a mapped drive's, like `\\?\C:\`.
    assert_eq!(
        plain_form(r"\\?\UNC\server\share\").as_deref(),
        Some(r"\\server\share\")
    );
}

#[test]
fn a_name_with_a_character_above_the_controls_a_plain_path_reads_loses_its_prefix() {
    // U+007F to U+009F are control characters, but legal in a plain name.
    for name in ["a\u{7f}b", "a\u{85}b", "a\u{9f}b"] {
        let text = format!(r"\\?\C:\dir\{name}");
        assert_eq!(
            plain_form(&text),
            Some(format!(r"C:\dir\{name}")),
            "{name:?}"
        );
    }
}

#[test]
fn a_child_is_exact_under_a_plain_folder() {
    let child_text = |dir: &str, name: &str| child(Path::new(dir), name);
    // A name a plain path would read otherwise takes the verbatim form.
    assert_eq!(
        child_text(r"C:\proj", "notes."),
        PathBuf::from(r"\\?\C:\proj\notes.")
    );
    assert_eq!(
        child_text(r"C:\proj", "aux.c"),
        PathBuf::from(r"\\?\C:\proj\aux.c")
    );
    assert_eq!(
        child_text(r"\\server\share", "x."),
        PathBuf::from(r"\\?\UNC\server\share\x.")
    );
    assert_eq!(child_text(r"C:\", "aux"), PathBuf::from(r"\\?\C:\aux"));
    // Any other is the plain join, the root's separator not doubled.
    assert_eq!(
        child_text(r"C:\proj", "notes"),
        PathBuf::from(r"C:\proj\notes")
    );
    assert_eq!(child_text(r"C:\", "proj"), PathBuf::from(r"C:\proj"));
    assert_eq!(
        child_text(r"\\server\share\", "x"),
        PathBuf::from(r"\\server\share\x")
    );
    // Under a verbatim folder it stays verbatim, and so does a child whose
    // plain path would pass MAX_PATH.
    assert_eq!(
        child_text(r"\\?\C:\proj.", "x"),
        PathBuf::from(r"\\?\C:\proj.\x")
    );
    let long = format!(r"C:\{}", "d".repeat(250));
    assert_eq!(
        child_text(&long, "a-long-name.gguf"),
        PathBuf::from(format!(r"\\?\{long}\a-long-name.gguf"))
    );
    // A Unix path is joined as Path::join joins it.
    assert_eq!(
        child_text("/home/me", "notes."),
        Path::new("/home/me").join("notes.")
    );
}

#[test]
fn a_child_is_what_canonical_gives_the_same_entry() {
    for name in ["notes", "notes.", "aux.c", "a b", "a "] {
        let verbatim = format!(r"\\?\C:\proj\{name}");
        let expected = plain_form(&verbatim).unwrap_or(verbatim);
        assert_eq!(
            child(Path::new(r"C:\proj"), name),
            PathBuf::from(expected),
            "{name:?}"
        );
    }
}

#[test]
fn a_join_is_exact_under_a_plain_folder() {
    assert_eq!(
        join(Path::new(r"C:\proj"), Path::new("notes.")),
        PathBuf::from(r"\\?\C:\proj\notes.")
    );
    // A `..` in a verbatim join is a name, so it leads nowhere.
    assert_eq!(
        join(Path::new(r"C:\proj"), Path::new("../aux")),
        PathBuf::from(r"\\?\C:\proj\..\aux")
    );
    // Names a plain path reads alike are joined as Path::join joins them.
    for relative in ["notes", "../notes", "./a"] {
        assert_eq!(
            join(Path::new(r"C:\proj"), Path::new(relative)),
            Path::new(r"C:\proj").join(relative),
            "{relative}"
        );
    }
    assert_eq!(
        join(Path::new("/home/me"), Path::new("notes.")),
        Path::new("/home/me").join("notes.")
    );
}

#[test]
fn a_path_in_either_form_is_inside_a_folder_in_either_form() {
    let rest = |path: &str, folder: &str| inside(Path::new(path), Path::new(folder));
    for (path, folder, under) in [
        (r"\\?\C:\proj\notes.", r"C:\proj", "notes."),
        (r"\\?\C:\proj\aux.c", r"C:\proj", "aux.c"),
        (r"C:\proj\sub\x", r"\\?\C:\proj", r"sub\x"),
        (r"C:\proj\x", r"C:\proj", "x"),
        (r"C:\proj\x", r"C:\", r"proj\x"),
        (r"\\?\UNC\nas\share\x.", r"\\nas\share\", "x."),
        (r"C:\proj", r"\\?\C:\proj", ""),
    ] {
        assert_eq!(
            rest(path, folder),
            Some(PathBuf::from(under)),
            "{path} in {folder}"
        );
    }
    // A file whose plain path would pass MAX_PATH keeps its prefix in a
    // stripped folder, and is still inside it.
    let long = format!(r"\\?\C:\proj\{}", "a".repeat(300));
    assert_eq!(
        rest(&long, r"C:\proj"),
        Some(PathBuf::from("a".repeat(300)))
    );

    for (path, folder) in [
        (r"C:\project\x", r"C:\proj"),
        (r"\\?\D:\proj\x", r"C:\proj"),
        (r"\\nas\other\x", r"\\nas\share"),
        (r"C:\proj", r"C:\proj\sub"),
    ] {
        assert_eq!(rest(path, folder), None, "{path} in {folder}");
    }
    assert_eq!(rest("/home/me/x", "/home/me"), Some(PathBuf::from("x")));
    assert_eq!(rest("/home/mex", "/home/me"), None);
}

#[test]
fn a_path_with_no_prefix_is_left_as_it_is() {
    for text in [r"C:\a\b", r"\\server\share\x", "/home/me/model.gguf", ""] {
        assert_eq!(plain_form(text), None, "{text}");
    }
}

#[test]
fn a_share_without_both_names_keeps_its_prefix() {
    for text in [
        r"\\?\UNC\",
        r"\\?\UNC\server",
        r"\\?\UNC\server\",
        r"\\?\UNC\\share",
    ] {
        assert_eq!(plain_form(text), None, "{text}");
    }
}

#[test]
fn a_verbatim_path_that_is_not_a_drive_or_a_share_keeps_its_prefix() {
    for text in [
        r"\\?\Volume{0b6b1c3e-0000-0000-0000-100000000000}\a",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\a",
        r"\\?\C:",
        r"\\?\C:a",
        r"\\?\1:\a",
        r"\\.\C:\a",
    ] {
        assert_eq!(plain_form(text), None, "{text}");
    }
}

#[test]
fn a_path_longer_than_max_path_keeps_its_prefix() {
    // MAX_PATH is 260 UTF-16 units with the terminating NUL, so the plain
    // form may be 259 long.
    let fits = format!(r"\\?\C:\{}", "a".repeat(259 - 3));
    assert_eq!(plain_form(&fits).map(|p| p.len()), Some(259));
    let too_long = format!(r"\\?\C:\{}", "a".repeat(260 - 3));
    assert_eq!(plain_form(&too_long), None);
    // Counted in UTF-16 units, not bytes: 200 `é` are 400 bytes, 200 units.
    let accented = format!(r"\\?\C:\{}", "é".repeat(200));
    assert!(plain_form(&accented).is_some());
}

#[test]
fn a_name_the_plain_form_would_read_differently_keeps_its_prefix() {
    for name in [
        "trailing.",
        "trailing ",
        ".",
        "..",
        "a<b",
        "a>b",
        "a:b",
        "a\"b",
        "a/b",
        "a|b",
        "a?b",
        "a*b",
        "a\u{1}b",
        "",
    ] {
        let text = format!(r"\\?\C:\dir\{name}\file");
        assert_eq!(plain_form(&text), None, "{name:?}");
    }
}

#[test]
fn a_reserved_device_name_keeps_its_prefix() {
    for name in [
        "CON",
        "con",
        "PRN",
        "AUX",
        "NUL",
        "COM1",
        "com9",
        "COM0",
        "LPT1",
        "LPT0",
        "COM\u{b9}",
        "LPT\u{b3}",
        "CONIN$",
        "CONOUT$",
        "nul.txt",
        "NUL.tar.gz",
        "CON .txt",
    ] {
        let text = format!(r"\\?\C:\dir\{name}");
        assert_eq!(plain_form(&text), None, "{name:?}");
    }
    // Only the whole name before its first dot is reserved.
    for name in ["CONSOLE", "COM10", "nullable.gguf", "a.con"] {
        let text = format!(r"\\?\C:\dir\{name}");
        assert!(plain_form(&text).is_some(), "{name:?}");
    }
}

#[test]
fn only_the_verbatim_form_is_verbatim() {
    assert!(is_verbatim(r"\\?\C:\a"));
    assert!(is_verbatim(r"\\?\UNC\server\share"));
    assert!(!is_verbatim(r"C:\a"));
    assert!(!is_verbatim(r"\\server\share"));
    assert!(!is_verbatim("/a/b"));
}

#[test]
fn a_canonical_path_is_never_verbatim() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("model.gguf");
    std::fs::write(&file, b"").expect("write");

    for path in [dir.path(), file.as_path()] {
        let resolved = canonical(path).expect("resolved");
        let text = resolved.to_str().expect("a UTF-8 temp path");
        assert!(!is_verbatim(text), "{text}");
        assert!(resolved.is_absolute(), "{text}");
        assert_eq!(canonical(&resolved).expect("resolved again"), resolved);
    }
}

#[test]
fn a_missing_path_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(canonical(dir.path().join("absent")).is_err());
}
