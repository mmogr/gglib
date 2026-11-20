//! The copy a command is handed to, the refusal and the note after an
//! upgrade, over a `PATH` of temporary directories and answers the test
//! gives, so nothing here runs anything or reads this machine's `PATH`.

use std::path::{Path, PathBuf};

use gglib_db::{NewerLibrary, SCHEMA_GENERATION};

use super::{Machine, Other, left_behind, refusal, refused};
use crate::handlers::config::copies::{Apps, Search};

/// A `gglib` in `dir/name`, which is made.
fn copy(dir: &Path, name: &str) -> PathBuf {
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).expect("its directory");
    let path = folder.join(format!("gglib{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&path, "never run").expect("a stand-in");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("its mode");
    }
    path
}

/// A search of the folders `on_path` are in, run from `running`.
fn search(on_path: &[&Path], running: &Path) -> Search {
    let dirs = on_path.iter().map(|bin| bin.parent().expect("a folder"));
    Search {
        path: std::env::join_paths(dirs).expect("a PATH"),
        running: Some(running.to_path_buf()),
        running_version: "9.9.9".to_owned(),
        apps: Apps::Bundles(Vec::new()),
    }
}

/// Another copy at `path` that opens `opens`.
fn other(path: &str, opens: Option<i64>) -> Other {
    Other {
        path: PathBuf::from(path),
        version: Some("1.2.3".to_owned()),
        opens,
    }
}

/// A machine whose running copy is on `PATH` or not, with `others`.
fn machine(on_path: bool, others: Vec<Other>) -> Machine {
    Machine {
        running: "/old/gglib".to_owned(),
        on_path,
        others,
    }
}

fn newer(found: i64) -> NewerLibrary {
    NewerLibrary {
        found,
        restore: Some(" Go back with mv.".to_owned()),
    }
}

#[test]
fn the_other_copies_are_asked_and_the_running_one_is_not() {
    let root = tempfile::tempdir().expect("a directory");
    let (mine, old, new) = (
        copy(root.path(), "a"),
        copy(root.path(), "b"),
        copy(root.path(), "c"),
    );
    let silent = copy(root.path(), "d");
    let version = |bin: &Path| (bin != silent).then(|| "1.2.3".to_owned());
    let generation = |bin: &Path, version: &str| {
        assert_eq!(version, "1.2.3");
        assert_ne!(bin, silent, "a copy with no version is not asked");
        Some(if bin == old { 10 } else { 12 })
    };

    let got = Machine::of(
        &search(&[&mine, &old, &new, &silent], &mine),
        &version,
        &generation,
    );

    assert!(got.on_path);
    assert_eq!(got.running, mine.display().to_string());
    let seen: Vec<_> = got.others.iter().map(|o| (&o.path, o.opens)).collect();
    assert_eq!(
        seen,
        [(&old, Some(10)), (&new, Some(12)), (&silent, None)],
        "every other copy, in PATH order"
    );
}

#[test]
fn a_build_run_from_its_folder_is_not_on_path() {
    let root = tempfile::tempdir().expect("a directory");
    let (installed, built) = (copy(root.path(), "bin"), copy(root.path(), "target"));

    let got = Machine::of(&search(&[&installed], &built), &|_| None, &|_, _| None);

    assert!(!got.on_path);
    assert_eq!(got.others.len(), 1);
}

#[test]
fn the_copy_chosen_opens_the_library_and_upgrades_it_least() {
    let got = machine(
        true,
        vec![
            other("/unknown", None),
            other("/older", Some(10)),
            other("/newest", Some(13)),
            other("/exact", Some(11)),
            other("/exact-later", Some(11)),
        ],
    );

    assert_eq!(got.opener(11).map(|o| &o.path), Some(&"/exact".into()));
    assert_eq!(got.opener(12).map(|o| &o.path), Some(&"/newest".into()));
    assert_eq!(got.opener(14), None);
}

#[test]
fn a_refusal_names_the_copy_that_opens_the_library_and_offers_no_backup() {
    let got = machine(true, vec![other("/new/gglib", Some(12))]);

    let said = refusal(&newer(12), &got, got.opener(12));

    assert!(said.contains("/old/gglib"), "{said}");
    assert!(
        said.contains(&format!(
            "generation 12; this build knows {SCHEMA_GENERATION}"
        )),
        "{said}"
    );
    assert!(
        said.contains("/new/gglib (1.2.3) opens it: run that one"),
        "{said}"
    );
    assert!(!said.contains("Go back"), "{said}");
    assert!(!said.contains("GGLIB_DATA_DIR"), "{said}");
}

#[test]
fn a_refusal_with_no_copy_to_run_says_so_and_offers_the_backup_last() {
    let got = machine(true, vec![other("/as-old/gglib", Some(10))]);

    let said = refusal(&newer(12), &got, got.opener(12));

    assert!(said.contains("No other gglib on PATH opens it"), "{said}");
    assert!(said.ends_with(" Go back with mv."), "{said}");
}

#[test]
fn a_copy_that_did_not_say_what_it_opens_is_named_and_no_backup_is_offered() {
    let got = machine(true, vec![other("/slow/gglib", None)]);

    let said = refusal(&newer(12), &got, got.opener(12));

    assert!(
        said.contains("/slow/gglib (1.2.3) did not say what it opens"),
        "{said}"
    );
    assert!(!said.contains("Go back"), "{said}");
}

#[test]
fn any_other_error_is_returned_as_it_is() {
    let error = anyhow::anyhow!("the disk is full");

    let got = refused(error, &crate::commands::Commands::Gui);

    assert_eq!(got.to_string(), "the disk is full");
}

#[test]
fn a_build_off_path_is_told_how_to_keep_a_library_of_its_own() {
    let got = machine(false, vec![other("/new/gglib", Some(12))]);

    let said = refusal(&newer(12), &got, got.opener(12));

    assert!(said.contains("/new/gglib (1.2.3) opens it"), "{said}");
    assert!(
        said.contains("GGLIB_DATA_DIR and GGLIB_DAEMON_PORT"),
        "{said}"
    );
}

#[test]
fn an_upgrade_names_only_the_copies_it_leaves_behind() {
    let others = [
        other("/old/gglib", Some(SCHEMA_GENERATION - 1)),
        other("/same/gglib", Some(SCHEMA_GENERATION)),
        other("/unknown/gglib", None),
    ];

    let said = left_behind(&others);

    assert_eq!(said.len(), 3, "{said:?}");
    assert!(said[0].contains("no longer opens it"), "{said:?}");
    assert_eq!(said[1], "  /old/gglib (1.2.3)");
    assert!(left_behind(&others[1..]).is_empty());
}
