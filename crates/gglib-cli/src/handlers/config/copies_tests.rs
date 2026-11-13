//! Copies found on a `PATH` made of temporary directories, and app folders
//! made the same way, so nothing here depends on this machine's own `PATH`
//! or `/Applications`. Only the `read_version` tests run anything.

use std::ffi::OsString;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::copy_version::read_version;
use super::{Apps, Search, lines};

/// A temporary directory to build a machine in.
struct Root(tempfile::TempDir);

impl Root {
    fn new() -> Self {
        Self(tempfile::tempdir().expect("a temporary directory"))
    }

    /// `rel` under the root, with its parent directories made.
    fn at(&self, rel: &str) -> PathBuf {
        let path = self.0.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("its directory");
        path
    }

    /// The directory `rel`, made.
    fn dir(&self, rel: &str) -> PathBuf {
        let path = self.0.path().join(rel);
        std::fs::create_dir_all(&path).expect("a directory");
        path
    }

    /// A script at `rel` with `body`, executable unless `mode` says not.
    fn script(&self, rel: &str, body: &str, mode: u32) -> PathBuf {
        let path = self.at(rel);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("its mode");
        path
    }

    /// A `gglib` stand-in at `rel`, never run by the [`lines`] tests.
    fn gglib(&self, rel: &str) -> PathBuf {
        self.script(rel, "exit 99", 0o755)
    }
}

/// A `PATH` of `dirs`, in order.
fn path_of(dirs: &[&Path]) -> OsString {
    std::env::join_paths(dirs).expect("a PATH")
}

/// A search of `path` with no desktop app locations.
fn search(path: OsString, running: Option<PathBuf>) -> Search {
    Search {
        path,
        running,
        running_version: "9.9.9".to_owned(),
        apps: Apps::Bundles(Vec::new()),
    }
}

/// A version reader that answers from `answers` by path, and the record of
/// whom it asked.
type Asked = Arc<Mutex<Vec<PathBuf>>>;

fn reader(answers: &[(&Path, &str)]) -> (impl Fn(&Path) -> Option<String> + use<>, Asked) {
    let asked = Asked::default();
    let record = Arc::clone(&asked);
    let answers: Vec<(PathBuf, String)> = answers
        .iter()
        .map(|(p, v)| (p.to_path_buf(), (*v).to_owned()))
        .collect();
    let read = move |bin: &Path| {
        record.lock().expect("the record").push(bin.to_path_buf());
        answers
            .iter()
            .find(|(p, _)| p == bin)
            .map(|(_, v)| v.clone())
    };
    (read, asked)
}

#[test]
fn copies_are_listed_in_path_order_and_the_first_is_the_one_gglib_runs() {
    let root = Root::new();
    let (a, b, c) = (root.dir("a"), root.dir("b"), root.dir("c"));
    let first = root.gglib("a/gglib");
    let second = root.gglib("c/gglib");
    let running = root.gglib("build/gglib");
    let (read, _) = reader(&[(&first, "0.34.0"), (&second, "0.33.0")]);

    let got = lines(
        &search(path_of(&[&a, &b, &c]), Some(running.clone())),
        &read,
    );

    assert_eq!(
        got,
        vec![
            format!(
                "gglib_on_path_1 = {} (0.34.0, runs as `gglib`)",
                first.display()
            ),
            format!("gglib_on_path_2 = {} (0.33.0, shadowed)", second.display()),
            format!("running_from = {}, not on PATH", running.display()),
            "desktop_app = none found".to_owned(),
        ]
    );
}

#[test]
fn the_running_copy_is_this_one_and_is_not_asked() {
    let root = Root::new();
    let (a, b) = (root.dir("a"), root.dir("b"));
    let first = root.gglib("a/gglib");
    let mine = root.gglib("b/gglib");
    let (read, asked) = reader(&[(&first, "0.34.0")]);

    let got = lines(&search(path_of(&[&a, &b]), Some(mine.clone())), &read);

    assert_eq!(
        got[..2],
        [
            format!(
                "gglib_on_path_1 = {} (0.34.0, runs as `gglib`)",
                first.display()
            ),
            format!(
                "gglib_on_path_2 = {} (9.9.9, shadowed, this one)",
                mine.display()
            ),
        ]
    );
    assert!(
        !got.iter().any(|line| line.starts_with("running_from")),
        "{got:?}"
    );
    assert_eq!(*asked.lock().expect("the record"), vec![first]);
}

#[test]
fn a_running_copy_reached_through_a_link_is_this_one() {
    let root = Root::new();
    let bin = root.dir("bin");
    let cellar = root.gglib("Cellar/gglib/0.35.0/bin/gglib");
    symlink("../Cellar/gglib/0.35.0/bin/gglib", bin.join("gglib")).expect("a link");
    let (read, _) = reader(&[]);

    let got = lines(&search(path_of(&[&bin]), Some(cellar)), &read);

    assert_eq!(
        got[0],
        format!(
            "gglib_on_path_1 = {} (9.9.9, runs as `gglib`, this one)",
            bin.join("gglib").display()
        )
    );
    assert_eq!(got.len(), 2, "{got:?}");
}

#[test]
fn a_repeated_entry_or_a_link_to_a_listed_file_is_listed_once_under_its_first_path() {
    let root = Root::new();
    let (a, b, c) = (root.dir("a"), root.dir("b"), root.dir("c"));
    let first = root.gglib("a/gglib");
    // `b` reaches `a`'s file through a link to the directory, `c` through a
    // link to the file; `a` is also on PATH twice.
    let linked_dir = root.0.path().join("a-again");
    symlink(&a, &linked_dir).expect("a directory link");
    symlink(&first, c.join("gglib")).expect("a file link");
    let other = root.gglib("b/gglib");
    let (read, _) = reader(&[(&first, "0.34.0"), (&other, "0.30.0")]);

    let got = lines(
        &search(path_of(&[&a, &a, &linked_dir, &c, &b]), None),
        &read,
    );

    assert_eq!(
        got,
        vec![
            format!(
                "gglib_on_path_1 = {} (0.34.0, runs as `gglib`)",
                first.display()
            ),
            format!("gglib_on_path_2 = {} (0.30.0, shadowed)", other.display()),
            "desktop_app = none found".to_owned(),
        ]
    );
}

#[test]
fn a_file_that_cannot_run_is_not_a_copy() {
    let root = Root::new();
    let (a, b, c) = (root.dir("a"), root.dir("b"), root.dir("c"));
    root.script("a/gglib", "exit 0", 0o644);
    root.dir("b/gglib");
    let runnable = root.gglib("c/gglib");
    let (read, _) = reader(&[]);

    let got = lines(&search(path_of(&[&a, &b, &c]), None), &read);

    assert_eq!(
        got[0],
        format!(
            "gglib_on_path_1 = {} (version unknown, runs as `gglib`)",
            runnable.display()
        )
    );
    assert_eq!(got.len(), 2, "{got:?}");
}

#[test]
fn an_empty_path_finds_nothing_and_says_where_this_one_runs_from() {
    let root = Root::new();
    let running = root.gglib("build/gglib");
    let (read, asked) = reader(&[]);

    let got = lines(&search(OsString::new(), Some(running.clone())), &read);

    assert_eq!(
        got,
        vec![
            "gglib_on_path = none found".to_owned(),
            format!("running_from = {}, not on PATH", running.display()),
            "desktop_app = none found".to_owned(),
        ]
    );
    assert!(asked.lock().expect("the record").is_empty());
}

#[test]
fn a_mac_app_reports_the_version_its_info_plist_carries() {
    let root = Root::new();
    let plist = root.at("Applications/gglib.app/Contents/Info.plist");
    std::fs::write(
        &plist,
        "<?xml version=\"1.0\"?>\n<plist version=\"1.0\"><dict>\n\
         <key>CFBundleName</key>\n<string>gglib</string>\n\
         <key>CFBundleShortVersionString</key>\n  <string>0.35.0</string>\n\
         </dict></plist>\n",
    )
    .expect("an Info.plist");
    // An older bundle whose plist has no version, and a name not installed.
    root.dir("home/Applications/GGLib GUI.app/Contents");
    let bundles = vec![
        root.0.path().join("Applications/gglib.app"),
        root.0.path().join("Applications/GGLib GUI.app"),
        root.0.path().join("home/Applications/GGLib GUI.app"),
    ];
    let (read, asked) = reader(&[]);
    let mut machine = search(OsString::new(), None);
    machine.apps = Apps::Bundles(bundles.clone());

    let got = lines(&machine, &read);

    assert_eq!(
        got,
        vec![
            "gglib_on_path = none found".to_owned(),
            format!("desktop_app_1 = {} (0.35.0)", bundles[0].display()),
            format!("desktop_app_2 = {} (version unknown)", bundles[2].display()),
        ]
    );
    assert!(
        asked.lock().expect("the record").is_empty(),
        "an app is never run"
    );
}

#[test]
fn a_linux_app_is_never_run_and_takes_the_version_of_the_gglib_beside_it() {
    let root = Root::new();
    let (usr, old) = (root.dir("usr/bin"), root.dir("opt/bin"));
    let cli = root.gglib("usr/bin/gglib");
    let app = root.gglib("usr/bin/gglib-gui");
    let old_app = root.gglib("opt/bin/gg-lib-gui");
    let (read, asked) = reader(&[(&cli, "0.35.0")]);
    let mut machine = search(path_of(&[&usr, &old]), None);
    machine.apps = Apps::OnPath(super::LINUX_APP_NAMES);

    let got = lines(&machine, &read);

    assert_eq!(
        got,
        vec![
            format!(
                "gglib_on_path_1 = {} (0.35.0, runs as `gglib`)",
                cli.display()
            ),
            format!("desktop_app_1 = {} (0.35.0)", app.display()),
            format!("desktop_app_2 = {} (version unknown)", old_app.display()),
        ]
    );
    let asked = asked.lock().expect("the record").clone();
    assert_eq!(asked, vec![cli], "only the CLI is run, and once");
}

#[test]
fn a_windows_install_folder_takes_the_version_of_the_gglib_inside() {
    let root = Root::new();
    let folder = root.dir("Local/gglib-gui");
    let cli = root.gglib("Local/gglib-gui/gglib");
    let (read, _) = reader(&[(&cli, "0.35.0")]);
    let mut machine = search(OsString::new(), None);
    machine.apps = Apps::Folders(vec![folder.clone(), root.0.path().join("missing")]);

    let got = lines(&machine, &read);

    assert_eq!(
        got[1..],
        [format!("desktop_app_1 = {} (0.35.0)", folder.display())]
    );
}

#[test]
fn a_copy_answers_its_version_and_anything_else_reads_as_unknown() {
    let root = Root::new();
    let answers = root.script("ok/gglib", "echo 'gglib 0.34.0 (a1b2c3d4e5f6)'", 0o755);
    let fails = root.script("fails/gglib", "echo 'gglib 0.34.0'; exit 1", 0o755);
    let noise = root.script("noise/gglib", "echo 'usage: something else'", 0o755);
    let reads = root.script(
        "reads/gglib",
        "if read -r line; then echo 'gglib 0.0.1'; else echo 'gglib 0.1.0'; fi",
        0o755,
    );
    let wait = Duration::from_secs(5);

    assert_eq!(read_version(&answers, wait).as_deref(), Some("0.34.0"));
    assert_eq!(read_version(&fails, wait), None);
    assert_eq!(read_version(&noise, wait), None);
    // Nothing on stdin: `read` meets the end of input at once.
    assert_eq!(read_version(&reads, wait).as_deref(), Some("0.1.0"));
    assert_eq!(read_version(&root.0.path().join("missing"), wait), None);
}

#[test]
fn a_copy_that_hangs_is_killed_and_reads_as_unknown() {
    let root = Root::new();
    let hangs = root.script("hangs/gglib", "sleep 30", 0o755);

    let started = Instant::now();
    let got = read_version(&hangs, Duration::from_millis(300));

    assert_eq!(got, None);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}
