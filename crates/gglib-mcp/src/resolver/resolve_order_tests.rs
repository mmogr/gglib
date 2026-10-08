//! The order candidates are tried in, and what is recorded, case by case.
//!
//! The Test dialog shows the attempts, so their order is behaviour. The paths
//! are Unix ones, and Windows tries each directory under every `PATHEXT` name,
//! so this runs on Unix only.

use crate::resolver::env::MockEnv;
use crate::resolver::fs::MockFs;
use crate::resolver::resolve::resolve_executable_with_deps;
use crate::resolver::types::{AttemptOutcome, ResolveError};
use std::path::{Path, PathBuf};

/// This platform's default directories, in the order they are tried.
const DEFAULT_DIRS: &[&str] = if cfg!(target_os = "macos") {
    &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
} else {
    &["/usr/local/bin", "/usr/bin", "/bin"]
};

/// The `PATH` the cases search: three directories and an empty entry.
const PATH: &str = "/gglib-test/a::/gglib-test/b:/gglib-test/c";
const PATH_DIRS: &[&str] = &["/gglib-test/a", "/gglib-test/b", "/gglib-test/c"];

/// The user's extra directories, an empty one among them.
const USER_DIRS: &[&str] = &["/gglib-test/u1", "", "/gglib-test/u2"];

/// What `/etc/paths` and `/etc/paths.d/*` name on this machine. On macOS the
/// resolver reads them from the real filesystem, so the expectation has to.
fn etc_paths_dirs() -> Vec<String> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let mut files = vec![PathBuf::from("/etc/paths")];
    if let Ok(entries) = std::fs::read_dir("/etc/paths.d") {
        files.extend(entries.flatten().map(|entry| entry.path()));
    }
    let mut dirs = Vec::new();
    for contents in files.iter().filter_map(|f| std::fs::read_to_string(f).ok()) {
        let lines = contents.lines().map(str::trim);
        dirs.extend(
            lines
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(String::from),
        );
    }
    dirs
}

/// A home directory with nvm in it: three versions, the middle one the default.
fn home_with_nvm() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let nvm = home.path().join(".nvm");
    for version in ["v18.0.0", "v20.1.0", "v22.3.0"] {
        std::fs::create_dir_all(nvm.join("versions/node").join(version)).unwrap();
    }
    std::fs::create_dir_all(nvm.join("alias")).unwrap();
    std::fs::write(nvm.join("alias/default"), "v20.1.0\n").unwrap();
    home
}

fn in_dirs<S: AsRef<str>>(dirs: &[S], command: &str) -> Vec<PathBuf> {
    dirs.iter()
        .filter(|dir| !dir.as_ref().is_empty())
        .map(|dir| Path::new(dir.as_ref()).join(command))
        .collect()
}

/// Every candidate for `command`, in the order they are tried when none of
/// them is there: `PATH`, `/etc/paths`, the default directories, the Node
/// version managers under `home` (asdf, volta, nvm's default, then nvm's
/// versions from the newest), then the user's directories.
fn search_order(command: &str, home: &Path) -> Vec<PathBuf> {
    let mut order = in_dirs(PATH_DIRS, command);
    order.extend(in_dirs(&etc_paths_dirs(), command));
    order.extend(in_dirs(DEFAULT_DIRS, command));
    if matches!(command, "npm" | "npx" | "node") {
        order.push(home.join(".asdf/shims").join(command));
        order.push(home.join(".volta/bin").join(command));
        for version in ["v20.1.0", "v22.3.0", "v20.1.0", "v18.0.0"] {
            let bin = home.join(".nvm/versions/node").join(version).join("bin");
            order.push(bin.join(command));
        }
    }
    order.extend(in_dirs(USER_DIRS, command));
    order
}

/// What is recorded when `executable` is the only one there: every candidate
/// before it found missing, then it, and nothing after.
fn recorded(order: &[PathBuf], executable: Option<&Path>) -> Vec<String> {
    let mut recorded = Vec::new();
    for candidate in order {
        let found = Some(candidate.as_path()) == executable;
        let outcome = if found {
            AttemptOutcome::Ok
        } else {
            AttemptOutcome::NotFound
        };
        recorded.push(format!("{}: {outcome}", candidate.display()));
        if found {
            break;
        }
    }
    recorded
}

fn env(home: &Path) -> MockEnv {
    MockEnv::new().with_var("PATH", PATH).with_var("HOME", home)
}

fn user_dirs() -> Vec<String> {
    USER_DIRS.iter().map(|dir| (*dir).to_string()).collect()
}

/// The resolver's answer for `command` when `executable` is the only one
/// there: the path it resolved to, if any, its attempts as it reports them (a
/// list on success, the lines of the error on failure), and its warnings.
fn resolve(
    command: &str,
    env: &MockEnv,
    executable: Option<&Path>,
) -> (Option<PathBuf>, Vec<String>, Vec<String>) {
    let fs = executable.map_or_else(MockFs::new, |path| MockFs::new().with_executable(path));

    match resolve_executable_with_deps(command, &user_dirs(), env, &fs) {
        Ok(result) => (
            Some(result.resolved_path),
            result
                .attempts
                .iter()
                .map(|a| format!("{}: {}", a.candidate.display(), a.outcome))
                .collect(),
            result.warnings,
        ),
        Err(ResolveError::NotResolved { attempts, .. }) => (
            None,
            attempts
                .lines()
                .map(|line| line.trim_start_matches("  ✗ ").to_string())
                .collect(),
            Vec::new(),
        ),
        Err(other) => panic!("{command}: {other}"),
    }
}

#[test]
fn a_command_is_looked_for_in_one_order_and_the_search_stops_at_the_first_hit() {
    let home = home_with_nvm();
    let home = home.path();
    let nvm = home.join(".nvm/versions/node");
    let [first_default, .., last_default] = DEFAULT_DIRS else {
        panic!("no default directories");
    };

    let cases: Vec<(&str, &str, Option<PathBuf>)> = vec![
        ("found on PATH", "tool", Some("/gglib-test/b/tool".into())),
        (
            "found in the first default directory",
            "tool",
            Some(Path::new(first_default).join("tool")),
        ),
        (
            "found in the last default directory",
            "tool",
            Some(Path::new(last_default).join("tool")),
        ),
        (
            "found through asdf",
            "node",
            Some(home.join(".asdf/shims/node")),
        ),
        (
            "found through volta",
            "npm",
            Some(home.join(".volta/bin/npm")),
        ),
        (
            "found through nvm's default version",
            "npx",
            Some(nvm.join("v20.1.0/bin/npx")),
        ),
        (
            "found through nvm's newest version",
            "npx",
            Some(nvm.join("v22.3.0/bin/npx")),
        ),
        (
            "found through nvm's oldest version",
            "npx",
            Some(nvm.join("v18.0.0/bin/npx")),
        ),
        (
            "found in the user's last directory",
            "tool",
            Some("/gglib-test/u2/tool".into()),
        ),
        (
            "found in the user's last directory, past the Node managers",
            "npx",
            Some("/gglib-test/u2/npx".into()),
        ),
        ("not found", "tool", None),
        ("not found, a Node command", "npx", None),
    ];

    for (name, command, executable) in cases {
        let executable = executable.as_deref();
        let order = search_order(command, home);

        let (resolved, attempts, warnings) = resolve(command, &env(home), executable);

        assert_eq!(resolved.as_deref(), executable, "{name}: resolved path");
        assert_eq!(attempts, recorded(&order, executable), "{name}: attempts");
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
    }
}

#[test]
fn an_absolute_path_that_is_there_is_the_only_attempt() {
    let home = home_with_nvm();
    let executable = Path::new("/gglib-test/elsewhere/tool");

    let (resolved, attempts, warnings) = resolve(
        "/gglib-test/elsewhere/tool",
        &env(home.path()),
        Some(executable),
    );

    assert_eq!(resolved.as_deref(), Some(executable));
    assert_eq!(attempts, ["/gglib-test/elsewhere/tool: OK"]);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn an_absolute_path_that_is_gone_is_recorded_then_its_name_is_searched_for() {
    let home = home_with_nvm();
    let env = env(home.path());
    let gone = "/gglib-test/gone/npx";
    let mut order = vec![PathBuf::from(gone)];
    order.extend(search_order("npx", home.path()));

    let executable = Path::new("/gglib-test/u1/npx");
    let (resolved, attempts, warnings) = resolve(gone, &env, Some(executable));

    assert_eq!(resolved.as_deref(), Some(executable));
    assert_eq!(attempts, recorded(&order, Some(executable)));
    assert_eq!(
        warnings,
        ["Absolute path '/gglib-test/gone/npx' failed (not found), \
             falling back to basename 'npx'"]
    );

    let nowhere =
        resolve_executable_with_deps(gone, &user_dirs(), &env, &MockFs::new()).unwrap_err();
    let tried: Vec<String> = recorded(&order, None)
        .iter()
        .map(|attempt| format!("  ✗ {attempt}"))
        .collect();
    assert_eq!(
        nowhere.to_string(),
        format!(
            "Could not resolve 'npx' to an executable path. Tried:\n{}",
            tried.join("\n")
        ),
        "the error names what was searched for, and lists the absolute path first"
    );
}

#[test]
fn a_path_or_a_home_that_is_not_set_is_not_searched() {
    let mut elsewhere = in_dirs(&etc_paths_dirs(), "npx");
    elsewhere.extend(in_dirs(DEFAULT_DIRS, "npx"));
    elsewhere.extend(in_dirs(USER_DIRS, "npx"));
    let mut with_path = in_dirs(PATH_DIRS, "npx");
    with_path.extend(elsewhere.clone());

    let (_, attempts, _) = resolve("npx", &MockEnv::new(), None);
    assert_eq!(attempts, recorded(&elsewhere, None));

    let (_, attempts, _) = resolve("npx", &MockEnv::new().with_var("PATH", PATH), None);
    assert_eq!(attempts, recorded(&with_path, None));
}
