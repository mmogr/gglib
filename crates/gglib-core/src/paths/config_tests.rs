//! Unit tests for [`super`].

use super::*;
use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};
use tempfile::tempdir;

/// Every assignment in `file`, as loading it into an environment reads them:
/// in order, to the end. A line the loader refuses fails the test, since it
/// is where loading would stop.
fn loaded(file: &Path) -> Vec<(String, String)> {
    let contents = fs::read_to_string(file).unwrap();
    dotenvy::from_read_iter(contents.as_bytes())
        .collect::<Result<_, _>>()
        .unwrap_or_else(|refused| panic!("the loader stops at {refused}, in:\n{contents}"))
}

fn owned(assignments: &[(&str, &str)]) -> Vec<(String, String)> {
    assignments
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn test_persist_models_dir_writes_env_file() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();

    // Capture and restore GGLIB_DATA_DIR in a guard to ensure cleanup
    let _env_guard = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());

    let models_dir = temp.path().join("models");
    persist_models_dir(&models_dir).unwrap();

    let env_contents = fs::read_to_string(temp.path().join(".env")).unwrap();
    assert!(env_contents.contains("GGLIB_MODELS_DIR"));
    assert_eq!(
        persisted_models_dir().as_deref(),
        Some(models_dir.to_string_lossy().as_ref())
    );

    // _env_guard will restore the original value when dropped
}

/// The quoting holds for every string of up to three of the characters the
/// parser gives a meaning to, and a letter: the line reads back to the
/// string, and the line after it is still read.
#[test]
fn every_short_string_of_the_parsers_own_characters_reads_back_as_it_was() {
    const ALPHABET: [char; 13] = [
        'a', ' ', '"', '\'', '\\', '$', '#', '=', '\n', '\r', '\t', '{', '}',
    ];
    let mut values = vec![String::new()];
    let mut from = 0;
    for _ in 0..3 {
        let until = values.len();
        for at in from..until {
            for c in ALPHABET {
                values.push(format!("{}{c}", values[at]));
            }
        }
        from = until;
    }
    assert_eq!(values.len(), 1 + 13 + 13 * 13 + 13 * 13 * 13);

    for value in &values {
        let file = format!("KEY={}\nAFTER=2\n", quoted(value));
        let read: Result<Vec<_>, _> = dotenvy::from_read_iter(file.as_bytes()).collect();
        assert_eq!(
            read.ok(),
            Some(owned(&[("KEY", value), ("AFTER", "2")])),
            "{value:?}, written {file:?}"
        );
    }
}

/// Whatever a directory is called, it is stored as one line that the loader
/// reads back to the same name, and the lines after it still load.
#[test]
fn a_stored_value_is_one_line_the_loader_reads_back_whatever_it_holds() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let _env_guard = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let file = temp.path().join(".env");

    for value in [
        "/srv/plain",
        "/srv/my models",
        "/srv/it's",
        "/srv/say \"hi\"",
        "/srv/$HOME/${HOME}",
        r"C:\Users\me\models\",
        "/srv/a#b #c",
        "/srv/a=b",
        "/srv/tab\there",
        "/srv/line\nbreak",
        "/srv/modèles 模型",
        " /srv/padded ",
    ] {
        fs::write(&file, "BEFORE=1\nGGLIB_MODELS_DIR=/old\nAFTER=2\n").unwrap();

        persist_env_value(MODELS_DIR_KEY, value).unwrap();

        assert_eq!(
            loaded(&file),
            owned(&[("BEFORE", "1"), (MODELS_DIR_KEY, value), ("AFTER", "2")]),
            "{value:?}"
        );
        let stored = fs::read_to_string(&file).unwrap();
        assert_eq!(stored.lines().count(), 3, "{value:?} in {stored:?}");
        assert_eq!(read_env_value(MODELS_DIR_KEY).as_deref(), Some(value));

        // One line, so the next save replaces all of it.
        persist_env_value(MODELS_DIR_KEY, "/srv/next").unwrap();
        assert_eq!(
            loaded(&file),
            owned(&[
                ("BEFORE", "1"),
                (MODELS_DIR_KEY, "/srv/next"),
                ("AFTER", "2")
            ]),
            "after {value:?}"
        );
    }
}

/// A save leaves one line for the key, where the first one was, however that
/// line was written: bare with a space in it, which the loader refuses, after
/// an `export`, with spaces round the `=`, or twice over. A key that only
/// begins or ends like it is another key's line.
#[test]
fn a_save_replaces_the_line_that_assigned_the_key_however_it_was_written() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let _env_guard = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let file = temp.path().join(".env");

    for line in [
        "GGLIB_MODELS_DIR=/old dir",
        "export GGLIB_MODELS_DIR=/old",
        "  GGLIB_MODELS_DIR = '/old'",
        "GGLIB_MODELS_DIR=/old\nGGLIB_MODELS_DIR=/older",
    ] {
        fs::write(&file, format!("BEFORE=1\n{line}\nAFTER=2\n")).unwrap();

        persist_env_value(MODELS_DIR_KEY, "/srv/new dir").unwrap();

        assert_eq!(
            loaded(&file),
            owned(&[
                ("BEFORE", "1"),
                (MODELS_DIR_KEY, "/srv/new dir"),
                ("AFTER", "2")
            ]),
            "{line:?}"
        );
    }

    fs::write(
        &file,
        "exportGGLIB_MODELS_DIR=/a\nGGLIB_MODELS_DIR_OLD=/b\n",
    )
    .unwrap();
    persist_env_value(MODELS_DIR_KEY, "/srv/new dir").unwrap();
    assert_eq!(
        loaded(&file),
        owned(&[
            ("exportGGLIB_MODELS_DIR", "/a"),
            ("GGLIB_MODELS_DIR_OLD", "/b"),
            (MODELS_DIR_KEY, "/srv/new dir"),
        ])
    );
}

/// The file is also one a person edits. What they wrote reads as the loader
/// reads it: quoted or bare, with a comment after it or an `export` before
/// it, from the first line for the key and from no other key's. A line the
/// loader refuses holds no value, and does not hide the lines below it.
#[test]
fn a_hand_written_value_reads_as_the_loader_reads_it() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let _env_guard = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    assert_eq!(persisted_models_dir(), None, "no file, no directory");

    for (file, stored) in [
        (
            "GGLIB_MODELS_DIR=\"/srv/my models\"\n",
            Some("/srv/my models"),
        ),
        (
            "GGLIB_MODELS_DIR='/srv/single one'\n",
            Some("/srv/single one"),
        ),
        ("GGLIB_MODELS_DIR=/srv/bare # a note\n", Some("/srv/bare")),
        (
            "export GGLIB_MODELS_DIR=/srv/exported\n",
            Some("/srv/exported"),
        ),
        (
            "OTHER=/a\nGGLIB_MODELS_DIR = /b \nGGLIB_MODELS_DIR=/c\n",
            Some("/b"),
        ),
        ("# GGLIB_MODELS_DIR=/commented\nOTHER=/a\n", None),
        ("GGLIB_MODELS_DIR=\n", None),
        ("GGLIB_MODELS_DIR=\" \"\n", None),
        ("GGLIB_MODELS_DIR=/srv/my models\n", None),
        ("GGLIB_MODELS_DIR=\"/srv/unclosed\n", None),
        (
            "OTHER=a b\nGGLIB_MODELS_DIR=/srv/below\n",
            Some("/srv/below"),
        ),
    ] {
        fs::write(temp.path().join(".env"), file).unwrap();
        assert_eq!(persisted_models_dir().as_deref(), stored, "{file:?}");
    }
}
