//! [`data_root`] refuses a test the per-user folder, and a gglib program
//! not.
//!
//! Each case runs in a child, this binary run again for one test, with a
//! home of its own and neither root variable: what a program says of itself
//! lasts for its process, and so would a root another test set.

use std::path::Path;
use std::process::Output;

use super::{
    PathError, data_root, refuses_the_per_user_folder, resource_root, this_is_a_gglib_program,
};
use crate::utils::process::{cmd, host};

/// Set by the parent, so that an ordinary run leaves a child a no-op.
const CHILD_ENV: &str = "GGLIB_TEST_REFUSAL_CHILD";

/// Printed by a child once it has checked everything.
const MARKER: &str = "THE-CHILD-CHECKED-THE-REFUSAL";

/// This binary run again for the one test `name`, with `home` as its home.
fn child(name: &str, home: &Path) -> Output {
    cmd(std::env::current_exe().expect("this binary's path"))
        .args([
            "--exact",
            &format!("paths::platform::tests::{name}"),
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, "1")
        .env("HOME", home)
        .env_remove("XDG_DATA_HOME")
        .env_remove("GGLIB_DATA_DIR")
        .env_remove("GGLIB_RESOURCE_DIR")
        .output()
        .expect("run this binary again for the child")
}

/// That the child ran its checks and none failed.
fn passed(out: &Output) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the child failed:\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains(MARKER), "the child did not run:\n{stdout}");
}

fn in_a_child() -> bool {
    std::env::var_os(CHILD_ENV).is_some()
}

#[test]
fn a_test_that_names_no_root_is_refused_and_nothing_is_made() {
    let home = tempfile::tempdir().expect("a home for the child");

    passed(&child("the_child_names_no_root", home.path()));

    let made: Vec<_> = std::fs::read_dir(home.path())
        .expect("the home is read")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    assert!(made.is_empty(), "something was made in the home: {made:?}");
}

/// What the refusal says, when `asked` is refused.
fn refusal<T>(asked: fn() -> Result<T, PathError>) -> String {
    let Err(panic) = std::panic::catch_unwind(asked) else {
        panic!("the per-user folder was answered to a test");
    };
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|said| (*said).to_owned()))
        .expect("a panic with a message")
}

#[test]
fn the_child_names_no_root() {
    if !in_a_child() {
        return;
    }
    let said = refusal(data_root);
    for fix in ["isolate_data_root", "GGLIB_DATA_DIR"] {
        assert!(
            said.contains(fix),
            "the refusal does not name {fix}: {said}"
        );
    }
    assert_eq!(refusal(resource_root), said);

    // What a test may call of a program's start changes nothing.
    host::prepare();
    assert_eq!(refusal(data_root), said);
    println!("{MARKER}");
}

#[test]
fn a_gglib_program_is_answered_the_per_user_folder() {
    let home = tempfile::tempdir().expect("a home for the child");

    passed(&child("the_child_is_a_gglib_program", home.path()));
}

#[test]
fn the_child_is_a_gglib_program() {
    if !in_a_child() {
        return;
    }
    assert!(refuses_the_per_user_folder());
    #[expect(
        clippy::disallowed_methods,
        reason = "the child stands in for a program's `main`, in a process of its own"
    )]
    this_is_a_gglib_program();
    assert!(!refuses_the_per_user_folder());

    // Off Windows the home in the environment names the folder, so it is
    // resolved for real. Windows asks the system for it, and resolving it
    // there would make the user's own.
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").expect("the parent set a home");
        let root = data_root().expect("a program's data root");
        assert!(root.starts_with(&home), "{}", root.display());
        assert!(root.ends_with("gglib") && root.is_dir());
    }
    println!("{MARKER}");
}
