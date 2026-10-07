//! `gglib config llama update`, on each kind of install it can meet.
//!
//! The binary runs over a data root the test laid out, with a `PATH` of
//! stand-ins, so "the tools are missing" and "the checkout has changes" are
//! states a test can make. The refusals are compared with the text
//! `gglib_runtime` gives them, which is the text the GUI's update shows: the
//! route's own test, in `gglib-axum`, compares it with the same values.
//!
//! No test here lets a build start. A refused update never asks, and one
//! that asks is answered by the end of input, which cancels, in every test
//! but one. That one says yes, and the stand-in git pulls nothing and fails,
//! which is where an update stops.

#![cfg(unix)]

#[path = "support/stub_tools.rs"]
mod stub_tools;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use gglib_runtime::llama::{LOCAL_CHANGES_CAUTION, UpdateRefusal, build_tool_install_lines};
use stub_tools::{Machine, one_at_a_time};

/// `gglib config llama update` over `root`, on `machine`, with nobody typing.
fn update(root: &Path, machine: &Machine) -> Output {
    update_typing(root, machine, None)
}

/// The same, with `typed` on its standard input, or the end of input when
/// there is none.
fn update_typing(root: &Path, machine: &Machine, typed: Option<&str>) -> Output {
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");
    let mut child = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["config", "llama", "update"])
        .current_dir(elsewhere.path())
        .env_clear()
        .env("PATH", machine.path())
        .env("HOME", root)
        .env("GGLIB_DATA_DIR", root)
        .env("GGLIB_RESOURCE_DIR", root)
        .stdin(typed.map_or_else(Stdio::null, |_| Stdio::piped()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("running `gglib config llama update`");

    if let Some(typed) = typed {
        let mut stdin = child.stdin.take().expect("its standard input");
        stdin.write_all(typed.as_bytes()).expect("typing at it");
    }
    child.wait_with_output().expect("waiting for the command")
}

fn text(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A `llama-server` where the launcher looks for one.
fn install_binary(root: &Path) {
    let bin = root.join(".llama/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("llama-server"), "not a program").unwrap();
}

/// A source checkout beside it, and the record a Metal build wrote.
fn install_checkout(root: &Path) {
    std::fs::create_dir_all(root.join(".llama/llama.cpp/.git")).unwrap();
    std::fs::write(
        root.join(".llama/llama-config.json"),
        r#"{
  "version": "abc1234",
  "commit_sha": "abc1234def5678",
  "build_date": "2026-08-10T12:34:56Z",
  "acceleration": "Metal",
  "cmake_flags": ["-DGGML_METAL=ON"]
}"#,
    )
    .unwrap();
}

/// The build tools, as stand-ins. `git status` prints `changes`, and
/// `git pull` fails, so that no update gets as far as a build.
fn build_tools(changes: &str) -> Machine {
    Machine::bare()
        .with(
            "git",
            &format!(
                "case \"$*\" in\n\
                 *status*) printf '%s' '{changes}' ;;\n\
                 *pull*) echo 'fatal: a stand-in pulls nothing' >&2; exit 1 ;;\n\
                 *) echo 'git version 2.43.0' ;;\n\
                 esac"
            ),
        )
        .with_tool("cmake", "cmake version 3.28.1")
        .with_tool("g++", "g++ (GCC) 14.2.1 20240910")
}

/// What a refused update prints: the refusal, as the command's error.
fn refused_with(out: &Output, refusal: &UpdateRefusal) {
    assert_eq!(out.status.code(), Some(1), "{}", text(out));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim_end(),
        format!("Error: {refusal}"),
        "{}",
        text(out)
    );
    assert!(out.stdout.is_empty(), "nothing is asked: {}", text(out));
}

#[test]
fn with_nothing_installed_an_update_is_refused_and_says_to_install() {
    let _guard = one_at_a_time();
    let root = tempfile::tempdir().unwrap();

    let out = update(root.path(), &build_tools(""));

    refused_with(&out, &UpdateRefusal::NotInstalled);
}

/// The advice is `rebuild`. It was `install`, which answers "already
/// installed" to this very state, and the command exited 0.
#[test]
fn a_pre_built_install_is_refused_and_says_to_rebuild() {
    let _guard = one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    install_binary(root.path());

    let out = update(root.path(), &build_tools(""));

    refused_with(&out, &UpdateRefusal::NoSourceCheckout);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("'gglib config llama rebuild'"),
        "{}",
        text(&out)
    );
}

#[test]
fn a_source_build_without_its_tools_is_refused_and_says_how_to_install_them() {
    let _guard = one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    install_binary(root.path());
    install_checkout(root.path());

    let out = update(root.path(), &Machine::bare());

    refused_with(
        &out,
        &UpdateRefusal::MissingTools(vec!["git", "cmake", "C++ compiler"]),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    for line in build_tool_install_lines() {
        assert!(stderr.contains(&line), "{line:?} in\n{}", text(&out));
    }
}

/// With its tools there, the update says what it will do and asks. Nobody
/// answers, so nothing is pulled.
#[test]
fn a_source_build_with_its_tools_may_update_and_asks_first() {
    let _guard = one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    install_binary(root.path());
    install_checkout(root.path());
    let machine = build_tools("");

    let out = update(root.path(), &machine);

    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Updating llama.cpp...\n\
         \n\
         Current version: abc1234\n\
         Build config: Metal\n\
         \n\
         This will:\n\
         \x20 - Pull latest llama.cpp changes\n\
         \x20 - Rebuild with Metal support\n\
         \x20 - Replace current binary\n\
         \n\
         Current models will NOT be affected.\n\
         \n\
         Continue? (y/N): \n\
         Update cancelled.\n",
        "{}",
        text(&out)
    );
    assert!(
        !machine.calls().iter().any(|call| call.contains("pull")),
        "nothing was pulled: {:?}",
        machine.calls()
    );
}

/// A changed checkout is not refused: git pulls over changes upstream left
/// alone. The user is told before being asked.
#[test]
fn a_checkout_with_local_changes_may_update_and_is_cautioned_first() {
    let _guard = one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    install_binary(root.path());
    install_checkout(root.path());

    let out = update(root.path(), &build_tools(" M src/llama.cpp"));

    assert!(out.status.success(), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!(
            "Current models will NOT be affected.\n\n{LOCAL_CHANGES_CAUTION}\n\nContinue? (y/N): \n"
        )),
        "{}",
        text(&out)
    );
    assert!(stdout.ends_with("Update cancelled.\n"), "{}", text(&out));
}

/// A directory with no `.git` of its own is not asked about: git would answer
/// for whichever repository the directory sits inside. Here it would answer
/// that there is a change.
#[test]
fn a_checkout_that_is_no_repository_of_its_own_is_not_asked_for_changes() {
    let _guard = one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    install_binary(root.path());
    install_checkout(root.path());
    std::fs::remove_dir(root.path().join(".llama/llama.cpp/.git")).unwrap();
    let machine = build_tools(" M src/llama.cpp");

    let out = update(root.path(), &machine);

    assert!(out.status.success(), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Current models will NOT be affected.\n\nContinue? (y/N): \n"),
        "{}",
        text(&out)
    );
    assert!(!stdout.contains(LOCAL_CHANGES_CAUTION), "{}", text(&out));
    let calls = machine.calls();
    assert!(calls.contains(&"git --version".to_owned()), "{calls:?}");
    assert!(!calls.iter().any(|c| c.contains("status")), "{calls:?}");
}

/// git has the terminal for a pull and reports it there, so the command says
/// that it is pulling and draws nothing across it. A line logged under a
/// spinner is drawn with it, and is not on stdout.
#[test]
fn an_update_that_is_agreed_to_announces_its_pull_and_leaves_it_to_git() {
    let _guard = one_at_a_time();
    let root = tempfile::tempdir().unwrap();
    install_binary(root.path());
    install_checkout(root.path());
    let machine = build_tools("");

    let out = update_typing(root.path(), &machine, Some("y\n"));

    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .ends_with("Continue? (y/N): \n\nPulling latest llama.cpp changes...\n"),
        "{}",
        text(&out)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "fatal: a stand-in pulls nothing\nError: Failed to pull updates from origin/master\n",
        "{}",
        text(&out)
    );
    let calls = machine.calls();
    let cmake_ran = |c: &String| c.starts_with("cmake") && c != "cmake --version";
    assert!(
        calls.iter().any(|c| c.ends_with(" pull origin master")),
        "{calls:?}"
    );
    assert!(!calls.iter().any(cmake_ran), "nothing was built: {calls:?}");
}
