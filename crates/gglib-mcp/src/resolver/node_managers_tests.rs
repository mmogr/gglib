//! Node's version managers are found where each keeps its programs on
//! Windows (#2012), with Windows' rules and environment passed in as values,
//! so these run on every OS.

use std::path::{Path, PathBuf};

use super::ExecutableSearcher;
use crate::resolver::env::MockEnv;
use crate::resolver::fs::MockFs;
use crate::resolver::platform::Platform;
use crate::resolver::types::{Attempt, AttemptOutcome};

const APPDATA: &str = r"C:\Users\u\AppData\Roaming";
const LOCALAPPDATA: &str = r"C:\Users\u\AppData\Local";

/// What Node's version managers find `command` as, on Windows, in `env`
/// with `fs`; and every candidate checked.
fn found_on_windows(env: &MockEnv, fs: &MockFs, command: &str) -> (Option<PathBuf>, Vec<Attempt>) {
    let windows = Platform::windows(None);
    let searcher = ExecutableSearcher::new(env, fs, &windows);
    let mut attempts = Vec::new();
    let found = searcher.search_node_managers(command, &mut attempts);
    (found, attempts)
}

/// A Windows user's environment: `APPDATA` and `LOCALAPPDATA`, and no `HOME`.
fn windows_env() -> MockEnv {
    MockEnv::new()
        .with_var("APPDATA", APPDATA)
        .with_var("LOCALAPPDATA", LOCALAPPDATA)
}

#[test]
fn volta_is_found_in_local_app_data() {
    let shim = Path::new(LOCALAPPDATA)
        .join("Volta")
        .join("bin")
        .join("npx.exe");
    let fs = MockFs::new().with_executable(&shim);

    assert_eq!(found_on_windows(&windows_env(), &fs, "npx").0, Some(shim));
}

#[test]
fn volta_is_found_in_volta_home() {
    let home = r"D:\tools\volta";
    let shim = Path::new(home).join("bin").join("node.exe");
    let fs = MockFs::new().with_executable(&shim);
    let env = windows_env().with_var("VOLTA_HOME", home);

    assert_eq!(found_on_windows(&env, &fs, "node").0, Some(shim));
}

#[test]
fn nvm_windows_is_found_through_its_link_to_the_version_in_use() {
    let link = r"C:\Program Files\nodejs";
    let npx = Path::new(link).join("npx.cmd");
    let fs = MockFs::new().with_executable(&npx);
    let env = windows_env().with_var("NVM_SYMLINK", link);

    assert_eq!(found_on_windows(&env, &fs, "npx").0, Some(npx));
}

/// nvm-windows keeps each version in a folder of its own, `v<version>`,
/// with `node.exe` and `npx.cmd` in it, under `NVM_HOME`.
#[test]
fn nvm_windows_is_found_in_its_newest_version_under_nvm_home() {
    let home = versions(&["v18.0.0", "v20.1.0"]);
    let npx = home.path().join("v20.1.0").join("npx.cmd");
    let fs = MockFs::new()
        .with_executable(&npx)
        .with_executable(home.path().join("v18.0.0").join("npx.cmd"));
    let env = windows_env().with_var("NVM_HOME", home.path());

    assert_eq!(found_on_windows(&env, &fs, "npx").0, Some(npx));
}

#[test]
fn nvm_windows_is_found_in_app_data_without_nvm_home() {
    let appdata = tempfile::tempdir().unwrap();
    let version = appdata.path().join("nvm").join("v20.1.0");
    std::fs::create_dir_all(&version).unwrap();
    let npm = version.join("npm.cmd");
    let fs = MockFs::new().with_executable(&npm);
    let env = MockEnv::new().with_var("APPDATA", appdata.path());

    assert_eq!(found_on_windows(&env, &fs, "npm").0, Some(npm));
}

/// fnm puts a shell's version on `PATH` as `FNM_MULTISHELL_PATH` itself on
/// Windows, not its `bin` folder as elsewhere.
#[test]
fn fnm_is_found_through_the_shells_version() {
    let shell = Path::new(LOCALAPPDATA).join("fnm_multishells").join("1_2");
    let node = shell.join("node.exe");
    let fs = MockFs::new().with_executable(&node);
    let env = windows_env().with_var("FNM_MULTISHELL_PATH", &shell);

    assert_eq!(found_on_windows(&env, &fs, "node").0, Some(node));
}

#[test]
fn fnm_is_found_through_its_default_version_in_app_data() {
    let node = Path::new(APPDATA)
        .join("fnm")
        .join("aliases")
        .join("default")
        .join("node.exe");
    let fs = MockFs::new().with_executable(&node);

    assert_eq!(found_on_windows(&windows_env(), &fs, "node").0, Some(node));
}

#[test]
fn fnm_is_found_through_its_default_version_in_fnm_dir() {
    let dir = r"D:\fnm";
    let node = Path::new(dir)
        .join("aliases")
        .join("default")
        .join("node.exe");
    let fs = MockFs::new().with_executable(&node);
    let env = windows_env().with_var("FNM_DIR", dir);

    assert_eq!(found_on_windows(&env, &fs, "node").0, Some(node));
}

/// With nothing installed, every manager's place is checked, each under
/// Windows' names for the command, and none is a Unix one.
#[test]
fn every_windows_place_is_checked_and_no_unix_one() {
    let env = windows_env().with_var("HOME", r"C:\Users\u");
    let (found, attempts) = found_on_windows(&env, &MockFs::new(), "npx");

    assert_eq!(found, None);
    assert!(
        attempts
            .iter()
            .all(|a| a.outcome == AttemptOutcome::NotFound),
        "{attempts:?}"
    );
    let checked: Vec<&Path> = attempts.iter().map(|a| a.candidate.as_path()).collect();
    for place in [
        Path::new(LOCALAPPDATA).join("Volta").join("bin"),
        Path::new(APPDATA)
            .join("fnm")
            .join("aliases")
            .join("default"),
    ] {
        assert!(
            checked.contains(&place.join("npx.cmd").as_path()),
            "{} was not checked: {checked:?}",
            place.display()
        );
    }
    assert!(
        checked.iter().all(
            |c| !c.to_string_lossy().contains(".asdf") && !c.to_string_lossy().contains(".nvm")
        ),
        "a Unix place was checked: {checked:?}"
    );
}

#[test]
fn a_command_that_is_not_nodes_is_not_looked_for() {
    let (found, attempts) = found_on_windows(&windows_env(), &MockFs::new(), "uvx");

    assert_eq!(found, None);
    assert!(attempts.is_empty(), "{attempts:?}");
}

/// A folder holding nvm-windows' version folders, `names`.
fn versions(names: &[&str]) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    for name in names {
        std::fs::create_dir_all(home.path().join(name)).unwrap();
    }
    // nvm-windows keeps its own program and settings beside the versions.
    std::fs::write(home.path().join("settings.txt"), "root: x\n").unwrap();
    home
}
