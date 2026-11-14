//! `gglib config paths` as the binary prints it, and what `gglib --version`
//! leaves behind, which is nothing. With help and `completions`, it opens no
//! library, so it makes none and answers beside one this build refuses.
//!
//! The binary runs with a data root and a home of its own and a `PATH` that
//! holds only directories the test made, so the only copies it asks for a
//! version are the stand-ins here. On a Mac it also reads the app bundles
//! in `/Applications`, so what the `desktop_app` lines say is not pinned.

use std::path::Path;
use std::process::{Command, Output};

/// `gglib <args>` with `data` as its data root, `home` as its home and
/// `path` as its whole `PATH`.
fn gglib(data: &Path, home: &Path, path: &[&Path], args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(args)
        .current_dir(home)
        .env("GGLIB_DATA_DIR", data)
        .env("HOME", home)
        .env("PATH", std::env::join_paths(path).expect("a PATH"))
        .env_remove("GGLIB_MODELS_DIR")
        .env_remove("GGLIB_RESOURCE_DIR")
        .output()
        .expect("gglib runs")
}

fn text(out: &Output) -> String {
    format!(
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// What `config paths` asks of another copy, `--version`, is safe to ask:
/// the answer comes before gglib makes its data root or anything in it.
#[test]
fn version_is_answered_before_anything_is_made() {
    let root = tempfile::tempdir().expect("a directory for it all");
    let (data, home) = (root.path().join("data"), root.path().join("home"));
    std::fs::create_dir(&home).expect("an empty home");

    let out = gglib(
        &data,
        &home,
        &[&root.path().join("no-path")],
        &["--version"],
    );

    assert!(out.status.success(), "{}", text(&out));
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.starts_with("gglib "), "{}", text(&out));
    assert!(!data.exists(), "the data root was made");
    let made: Vec<_> = std::fs::read_dir(&home)
        .expect("the home is read")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    assert!(made.is_empty(), "something was made in the home: {made:?}");
}

/// The library under the data root `data`.
fn database(data: &Path) -> std::path::PathBuf {
    data.join("data").join("gglib.db")
}

/// The files beside the library `db` that belong to it: the file itself,
/// its `-wal` and `-shm`, and any backup.
fn library_files(db: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(db.parent().expect("a parent")) else {
        return Vec::new();
    };
    entries
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("gglib.db"))
        })
        .collect()
}

/// Help, `completions` and `config paths` read nothing from the library,
/// so on a fresh data root they answer without making one.
#[test]
fn commands_that_read_no_library_make_none() {
    let root = tempfile::tempdir().expect("a directory for it all");
    let (data, home) = (root.path().join("data"), root.path().join("home"));
    std::fs::create_dir(&home).expect("a home");
    let no_path = root.path().join("no-path");

    for args in [&[][..], &["completions", "zsh"], &["config", "paths"]] {
        let out = gglib(&data, &home, &[&no_path], args);

        assert!(out.status.success(), "gglib {args:?}\n{}", text(&out));
        assert!(!out.stdout.is_empty(), "gglib {args:?}\n{}", text(&out));
        let made = library_files(&database(&data));
        assert!(made.is_empty(), "gglib {args:?} made {made:?}");
    }
}

/// `config paths` names a library this build refuses, which is what
/// `docs/compatibility.md` tells that library's owner to run, and leaves it
/// as it was.
#[test]
fn config_paths_names_a_library_this_build_refuses() {
    let root = tempfile::tempdir().expect("a directory for it all");
    let (data, home) = (root.path().join("data"), root.path().join("home"));
    std::fs::create_dir(&home).expect("a home");
    let no_path = root.path().join("no-path");
    let db = database(&data);
    std::fs::create_dir_all(db.parent().expect("a parent")).expect("the data folder");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(gglib_db::write_newer_database(&db))
        .expect("a newer library");

    // A command that does read the library is refused it.
    let show = gglib(&data, &home, &[&no_path], &["config", "show"]);
    assert!(!show.status.success(), "{}", text(&show));
    assert!(
        String::from_utf8_lossy(&show.stderr).contains("newer gglib"),
        "{}",
        text(&show)
    );
    let files = library_files(&db);
    let written = std::fs::read(&db).expect("the library is read");

    let out = gglib(&data, &home, &[&no_path], &["config", "paths"]);

    assert!(out.status.success(), "{}", text(&out));
    let line = format!("database_path = {}", db.display());
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|said| said == line),
        "no `{line}`\n{}",
        text(&out)
    );
    assert_eq!(library_files(&db), files, "beside the library");
    assert_eq!(
        std::fs::read(&db).expect("the library is read"),
        written,
        "the library changed"
    );
}

/// The keys `config paths` printed before it listed copies, in order.
#[cfg(unix)]
const OLD_KEYS: [&str; 11] = [
    "binary",
    "version",
    "data_root_source",
    "daemon_port",
    "data_root",
    "resource_root",
    "database_path",
    "llama_server_path",
    "sd_server_path",
    "models_dir",
    "models_source",
];

/// Whether `key` is `name`, or `name_N`.
#[cfg(unix)]
fn numbered(key: &str, name: &str) -> bool {
    key.strip_prefix(name).is_some_and(|rest| {
        rest.is_empty()
            || rest
                .strip_prefix('_')
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// A `gglib` stand-in in the new directory `dir`: a script that runs `body`.
#[cfg(unix)]
fn stub(dir: &Path, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::create_dir_all(dir).expect("a directory");
    let path = dir.join("gglib");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a stand-in");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("it is made executable");
    path
}

/// The lines every release has printed come first, each once and in their
/// order; the lines for the copies on `PATH` and the desktop app come after
/// them, under keys of their own. A copy that hangs costs its two seconds
/// and reads as `version unknown`.
#[cfg(unix)]
#[test]
fn the_old_lines_come_first_and_each_copy_on_path_follows() {
    use std::time::{Duration, Instant};

    let root = tempfile::tempdir().expect("a directory for it all");
    let (data, home) = (root.path().join("data"), root.path().join("home"));
    std::fs::create_dir(&home).expect("a home");
    let (first, second) = (root.path().join("a"), root.path().join("b"));
    let answers = stub(&first, "echo 'gglib 0.34.0 (a1b2c3d4e5f6)'");
    // This machine's sleep, by its full path: the PATH below has no tools.
    let sleep = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join("sleep"))
                .find(|candidate| candidate.is_file())
        })
        .expect("this machine has sleep");
    let hangs = stub(&second, &format!("exec '{}' 30", sleep.display()));

    let started = Instant::now();
    let out = gglib(&data, &home, &[&first, &second], &["config", "paths"]);
    let took = started.elapsed();

    assert!(out.status.success(), "{}", text(&out));
    assert!(took < Duration::from_secs(10), "took {took:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let pairs: Vec<(&str, &str)> = stdout
        .lines()
        .map(|line| {
            line.split_once(" = ")
                .unwrap_or_else(|| panic!("`{line}` is a `key = value` line"))
        })
        .collect();
    let keys: Vec<&str> = pairs.iter().map(|(key, _)| *key).collect();
    for (index, key) in keys.iter().enumerate() {
        assert!(!keys[..index].contains(key), "{key} is printed twice");
    }

    assert_eq!(keys[..OLD_KEYS.len()], OLD_KEYS, "{stdout}");
    let mut rest = &pairs[OLD_KEYS.len()..];
    if rest
        .first()
        .is_some_and(|(key, _)| *key == "database_backup")
    {
        rest = &rest[1..];
    }
    for (key, _) in rest {
        assert!(
            numbered(key, "gglib_on_path")
                || *key == "running_from"
                || numbered(key, "desktop_app"),
            "{key} is a key for a copy\n{stdout}"
        );
    }
    let copies: Vec<String> = rest
        .iter()
        .filter(|(key, _)| !numbered(key, "desktop_app"))
        .map(|(key, value)| format!("{key} = {value}"))
        .collect();
    let binary = pairs[0].1;
    assert_eq!(
        copies,
        [
            format!(
                "gglib_on_path_1 = {} (0.34.0, runs as `gglib`)",
                answers.display()
            ),
            format!(
                "gglib_on_path_2 = {} (version unknown, shadowed)",
                hangs.display()
            ),
            format!("running_from = {binary}, not on PATH"),
        ],
        "{stdout}"
    );
    assert!(
        rest.iter().any(|(key, _)| numbered(key, "desktop_app")),
        "the desktop app has a line\n{stdout}"
    );
}
