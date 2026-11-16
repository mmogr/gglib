//! The rules a program is found by, Windows's among them, tested on every OS.
//!
//! The rules are a value, so this machine's OS does not decide which are
//! tested: the Windows cases pass [`Platform::windows`] and look at real files
//! in a temporary folder. None of those files has an execute bit, so a Unix
//! permission check could not be what finds them.

use super::*;
use crate::resolver::env::MockEnv;
use crate::resolver::fs::{FsProvider, SystemFs};
use crate::resolver::resolve::resolve_executable_with_deps;
use crate::resolver::types::ResolveError;
use std::path::PathBuf;

/// A folder holding `files`, each an empty file.
fn folder_with(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for file in files {
        std::fs::write(dir.path().join(file), "").unwrap();
    }
    dir
}

/// What `command` resolves to under `platform` when `PATH` is `dir` alone,
/// and what was tried, each attempt named by its path inside `dir`.
fn resolve_in(command: &str, dir: &Path, platform: &Platform) -> (Option<PathBuf>, Vec<String>) {
    let env = MockEnv::new().with_var("PATH", dir);
    let attempt = |candidate: &Path, outcome: String| {
        let name = candidate.strip_prefix(dir).unwrap_or(candidate);
        format!("{}: {outcome}", name.display())
    };
    match resolve_executable_with_deps(command, &[], &env, &SystemFs, platform) {
        Ok(found) => (
            Some(found.resolved_path),
            found
                .attempts
                .iter()
                .map(|a| attempt(&a.candidate, a.outcome.to_string()))
                .collect(),
        ),
        Err(ResolveError::NotResolved { attempts, .. }) => (
            None,
            attempts
                .lines()
                .map(|line| line.trim_start_matches("  ✗ "))
                .map(|line| {
                    let (candidate, outcome) = line.rsplit_once(": ").unwrap();
                    attempt(Path::new(candidate), outcome.to_string())
                })
                .collect(),
        ),
        Err(other) => panic!("{command}: {other}"),
    }
}

#[test]
fn the_names_a_command_is_looked_for_under_are_its_extensions_then_itself() {
    let windows = Platform::windows(None);
    assert_eq!(
        windows.names("npx"),
        ["npx.com", "npx.exe", "npx.bat", "npx.cmd", "npx"],
        "Windows's own order when PATHEXT is not set"
    );
    assert_eq!(
        windows.names("npx.CMD"),
        ["npx.CMD"],
        "a command written with its extension is looked for as written"
    );
    let pathext = Platform::windows(Some(OsStr::new(".EXE;;.CMD")));
    assert_eq!(pathext.names("tool"), ["tool.exe", "tool.cmd", "tool"]);
    assert_eq!(Platform::unix().names("npx"), ["npx"]);
    assert_eq!(Platform::unix().names("tool.exe"), ["tool.exe"]);
}

/// Node's folder holds a bare `npx` shell script beside `npx.cmd`; Windows
/// can start only the second (B8 in #1959: os error 193).
#[test]
fn on_windows_a_command_is_found_by_its_extensions_before_its_bare_name() {
    let dir = folder_with(&["npx", "npx.cmd"]);

    let (resolved, attempts) = resolve_in("npx", dir.path(), &Platform::windows(None));

    assert_eq!(resolved, Some(dir.path().join("npx.cmd")));
    assert_eq!(
        attempts,
        [
            "npx.com: not found",
            "npx.exe: not found",
            "npx.bat: not found",
            "npx.cmd: OK"
        ]
    );
}

#[test]
fn on_windows_a_bare_file_is_not_a_program() {
    let dir = folder_with(&["npx", "tool.CMD"]);
    let windows = Platform::windows(None);

    let (resolved, attempts) = resolve_in("npx", dir.path(), &windows);

    assert_eq!(resolved, None);
    assert_eq!(
        attempts,
        [
            "npx.com: not found",
            "npx.exe: not found",
            "npx.bat: not found",
            "npx.cmd: not found",
            "npx: not executable"
        ]
    );
    let check = |name: &str| SystemFs.check_executable(&dir.path().join(name), &windows);
    assert_eq!(
        check("tool.CMD"),
        AttemptOutcome::Ok,
        "by extension, any case"
    );
    assert_eq!(check("npx"), AttemptOutcome::NotExecutable);
    assert_eq!(check("gone.exe"), AttemptOutcome::NotFound);
    assert_eq!(
        SystemFs.check_executable(dir.path(), &windows),
        AttemptOutcome::NotAFile
    );
}

#[test]
fn on_windows_the_extensions_are_tried_in_pathext_order_whatever_its_case() {
    let dir = folder_with(&["tool.exe", "tool.cmd"]);
    let with = |pathext: &str| Platform::windows(Some(OsStr::new(pathext)));

    let (resolved, _) = resolve_in("tool", dir.path(), &with(".CMD;.EXE"));
    assert_eq!(resolved, Some(dir.path().join("tool.cmd")));

    let (resolved, _) = resolve_in("tool", dir.path(), &with(".EXE;.CMD"));
    assert_eq!(resolved, Some(dir.path().join("tool.exe")));

    let (resolved, attempts) = resolve_in("tool", dir.path(), &with(".BAT"));
    assert_eq!(
        resolved, None,
        "an extension PATHEXT leaves out is not a program"
    );
    assert_eq!(attempts, ["tool.bat: not found", "tool: not found"]);
}

#[test]
fn on_windows_a_command_written_with_its_extension_is_looked_for_as_written() {
    let dir = folder_with(&["npx.cmd"]);

    let (resolved, attempts) = resolve_in("npx.cmd", dir.path(), &Platform::windows(None));

    assert_eq!(resolved, Some(dir.path().join("npx.cmd")));
    assert_eq!(attempts, ["npx.cmd: OK"]);
}

/// A path stored by an older gglib, or typed without its extension.
#[test]
fn on_windows_an_absolute_command_is_found_by_its_extensions_too() {
    let dir = folder_with(&["npx", "npx.cmd"]);
    let command = dir.path().join("npx");

    let found = resolve_executable_with_deps(
        command.to_str().unwrap(),
        &[],
        &MockEnv::new(),
        &SystemFs,
        &Platform::windows(None),
    )
    .unwrap();

    assert_eq!(found.resolved_path, dir.path().join("npx.cmd"));
    assert!(found.warnings.is_empty(), "{:?}", found.warnings);
}

#[test]
fn a_windows_list_is_split_on_semicolons_and_a_unix_list_on_colons() {
    let windows = Platform::windows(None);
    let split: Vec<&str> = windows.split(r"C:\tools;;D:\bin").collect();
    assert_eq!(split, [r"C:\tools", r"D:\bin"]);
    assert_eq!(windows.join(&split), r"C:\tools;D:\bin");

    let unix = Platform::unix();
    let split: Vec<&str> = unix.split("/opt/a::/opt/b").collect();
    assert_eq!(split, ["/opt/a", "/opt/b"]);
    assert_eq!(unix.join(&split), "/opt/a:/opt/b");
}

/// The separator is written down here rather than taken from
/// `std::env::split_paths`, so that Windows's can be tested on every OS. This
/// keeps the two the same for this machine.
#[test]
fn this_machines_lists_split_as_the_standard_library_splits_them() {
    let dirs = [
        std::env::temp_dir().join("gglib-test-a"),
        std::env::temp_dir().join("gglib-test-b"),
    ];
    let list = std::env::join_paths(&dirs).unwrap();

    let split: Vec<PathBuf> = Platform::host()
        .split(list.to_str().unwrap())
        .map(PathBuf::from)
        .collect();

    assert_eq!(split, std::env::split_paths(&list).collect::<Vec<_>>());
    assert_eq!(split, dirs);
}
