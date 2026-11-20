//! The order candidates are tried in, and what is recorded, case by case.
//!
//! The Test dialog shows the attempts, so their order is behaviour. Each OS's
//! rules are a value, so Unix's order and Windows' are both pinned on every
//! OS, over a mock environment and filesystem. What a machine adds of its
//! own, the default directories and on macOS `/etc/paths`, is pinned under
//! this machine's rules, on Unix.

use crate::resolver::env::MockEnv;
use crate::resolver::fs::MockFs;
use crate::resolver::platform::Platform;
use crate::resolver::resolve::resolve_executable_with_deps;
use crate::resolver::types::{AttemptOutcome, ResolveError};
use std::path::{Path, PathBuf};

/// The `PATH` the Unix cases search: three directories and an empty entry.
const PATH: &str = "/gglib-test/a::/gglib-test/b:/gglib-test/c";
const PATH_DIRS: &[&str] = &["/gglib-test/a", "/gglib-test/b", "/gglib-test/c"];

/// The user's extra directories, an empty one among them.
const USER_DIRS: &[&str] = &["/gglib-test/u1", "", "/gglib-test/u2"];

/// The `PATH` the Windows cases search: two directories and an empty entry.
const WINDOWS_PATH: &str = r"C:\gglib-test\a;;C:\gglib-test\b";
const WINDOWS_PATH_DIRS: &[&str] = &[r"C:\gglib-test\a", r"C:\gglib-test\b"];

/// The user's extra directories in the Windows cases, an empty one among them.
const WINDOWS_USER_DIRS: &[&str] = &[r"D:\gglib-test\u1", "", r"D:\gglib-test\u2"];

/// Where a Node version manager keeps its programs in the Windows cases:
/// what `NVM_SYMLINK` names.
const WINDOWS_NODE_DIR: &str = r"C:\gglib-test\nodejs";

/// A path that is absolute on this machine, whatever its OS: `/gglib-test/…`
/// is not absolute on Windows, which wants a drive.
fn absolute(parts: &[&str]) -> PathBuf {
    let mut path = std::env::temp_dir().join("gglib-test");
    path.extend(parts);
    path
}

/// The one name Unix's rules look for `command` under.
fn unix_names(command: &str) -> Vec<String> {
    vec![command.to_string()]
}

/// The names Windows' rules look for `command` under when `PATHEXT` is not
/// set, in the order they are tried.
fn windows_names(command: &str) -> Vec<String> {
    [".com", ".exe", ".bat", ".cmd", ""]
        .iter()
        .map(|extension| format!("{command}{extension}"))
        .collect()
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

/// Each of `dirs` in turn under every one of `names`, an empty directory
/// left out.
fn in_dirs<S: AsRef<str>>(dirs: &[S], names: &[String]) -> Vec<PathBuf> {
    dirs.iter()
        .filter(|dir| !dir.as_ref().is_empty())
        .flat_map(|dir| {
            names
                .iter()
                .map(move |name| Path::new(dir.as_ref()).join(name))
        })
        .collect()
}

/// Every candidate for `command` under Unix's rules, in the order they are
/// tried when none of them is there: `PATH`, the directories a machine adds
/// of its own (`machine`; [`Platform::unix`] adds none), the Node version
/// managers under `home` (asdf, volta, nvm's default, then nvm's versions
/// from the newest), then the user's directories.
fn unix_order(command: &str, home: &Path, machine: &[String]) -> Vec<PathBuf> {
    let names = unix_names(command);
    let mut order = in_dirs(PATH_DIRS, &names);
    order.extend(in_dirs(machine, &names));
    if matches!(command, "npm" | "npx" | "node") {
        order.push(home.join(".asdf/shims").join(command));
        order.push(home.join(".volta/bin").join(command));
        for version in ["v20.1.0", "v22.3.0", "v20.1.0", "v18.0.0"] {
            let bin = home.join(".nvm/versions/node").join(version).join("bin");
            order.push(bin.join(command));
        }
    }
    order.extend(in_dirs(USER_DIRS, &names));
    order
}

/// Every candidate for `command` under Windows' rules, in the order they are
/// tried when none of them is there: each `PATH` directory under every name,
/// then for Node's commands the version manager's directory, then the user's
/// directories. Windows has no default directories.
fn windows_order(command: &str) -> Vec<PathBuf> {
    let names = windows_names(command);
    let mut order = in_dirs(WINDOWS_PATH_DIRS, &names);
    if matches!(command, "npm" | "npx" | "node") {
        order.extend(in_dirs(&[WINDOWS_NODE_DIR], &names));
    }
    order.extend(in_dirs(WINDOWS_USER_DIRS, &names));
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

/// A Unix environment: the `PATH` of the cases, and `home`.
fn env(home: &Path) -> MockEnv {
    MockEnv::new().with_var("PATH", PATH).with_var("HOME", home)
}

/// A Windows environment: the `PATH` of the cases and a Node version
/// manager's directory, and no `HOME`.
fn windows_env() -> MockEnv {
    MockEnv::new()
        .with_var("PATH", WINDOWS_PATH)
        .with_var("NVM_SYMLINK", WINDOWS_NODE_DIR)
}

/// The resolver's answer for `command` under `platform`'s rules when
/// `executable` is the only one there: the path it resolved to, if any, its
/// attempts as it reports them (a list on success, the lines of the error on
/// failure), and its warnings.
fn resolve(
    command: &str,
    user_dirs: &[&str],
    env: &MockEnv,
    executable: Option<&Path>,
    platform: &Platform,
) -> (Option<PathBuf>, Vec<String>, Vec<String>) {
    let fs = executable.map_or_else(MockFs::new, |path| MockFs::new().with_executable(path));
    let user_dirs: Vec<String> = user_dirs.iter().map(|dir| (*dir).to_string()).collect();

    match resolve_executable_with_deps(command, &user_dirs, env, &fs, platform) {
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

/// What is said when the absolute path `gone` is not there and its name is
/// searched for.
fn fell_back(gone: &Path, name: &str) -> String {
    format!(
        "Absolute path '{}' failed (not found), falling back to basename '{name}'",
        gone.display()
    )
}

#[test]
fn a_command_is_looked_for_in_one_order_and_the_search_stops_at_the_first_hit() {
    let home = home_with_nvm();
    let home = home.path();
    let nvm = |version: &str| home.join(".nvm/versions/node").join(version).join("bin");

    let cases: Vec<(&str, &str, Option<PathBuf>)> = vec![
        (
            "found on PATH",
            "tool",
            Some(Path::new("/gglib-test/b").join("tool")),
        ),
        (
            "found through asdf",
            "node",
            Some(home.join(".asdf/shims").join("node")),
        ),
        (
            "found through volta",
            "npm",
            Some(home.join(".volta/bin").join("npm")),
        ),
        (
            "found through nvm's default version",
            "npx",
            Some(nvm("v20.1.0").join("npx")),
        ),
        (
            "found through nvm's newest version",
            "npx",
            Some(nvm("v22.3.0").join("npx")),
        ),
        (
            "found through nvm's oldest version",
            "npx",
            Some(nvm("v18.0.0").join("npx")),
        ),
        (
            "found in the user's last directory",
            "tool",
            Some(Path::new("/gglib-test/u2").join("tool")),
        ),
        (
            "found in the user's last directory, past the Node managers",
            "npx",
            Some(Path::new("/gglib-test/u2").join("npx")),
        ),
        ("not found", "tool", None),
        ("not found, a Node command", "npx", None),
    ];

    for (name, command, executable) in cases {
        let executable = executable.as_deref();
        let order = unix_order(command, home, &[]);

        let (resolved, attempts, warnings) = resolve(
            command,
            USER_DIRS,
            &env(home),
            executable,
            &Platform::unix(),
        );

        assert_eq!(resolved.as_deref(), executable, "{name}: resolved path");
        assert_eq!(attempts, recorded(&order, executable), "{name}: attempts");
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
    }
}

#[test]
fn an_absolute_path_that_is_there_is_the_only_attempt() {
    let home = home_with_nvm();
    let executable = absolute(&["elsewhere", "tool"]);

    let (resolved, attempts, warnings) = resolve(
        executable.to_str().unwrap(),
        USER_DIRS,
        &env(home.path()),
        Some(&executable),
        &Platform::unix(),
    );

    assert_eq!(resolved.as_deref(), Some(executable.as_path()));
    assert_eq!(attempts, [format!("{}: OK", executable.display())]);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn an_absolute_path_that_is_gone_is_recorded_then_its_name_is_searched_for() {
    let home = home_with_nvm();
    let env = env(home.path());
    let gone = absolute(&["gone", "npx"]);
    let mut order = vec![gone.clone()];
    order.extend(unix_order("npx", home.path(), &[]));

    let executable = Path::new("/gglib-test/u1").join("npx");
    let (resolved, attempts, warnings) = resolve(
        gone.to_str().unwrap(),
        USER_DIRS,
        &env,
        Some(&executable),
        &Platform::unix(),
    );

    assert_eq!(resolved.as_deref(), Some(executable.as_path()));
    assert_eq!(attempts, recorded(&order, Some(&executable)));
    assert_eq!(warnings, [fell_back(&gone, "npx")]);

    let user_dirs: Vec<String> = USER_DIRS.iter().map(|dir| (*dir).to_string()).collect();
    let nowhere = resolve_executable_with_deps(
        gone.to_str().unwrap(),
        &user_dirs,
        &env,
        &MockFs::new(),
        &Platform::unix(),
    )
    .unwrap_err();
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
    let names = unix_names("npx");
    let elsewhere = in_dirs(USER_DIRS, &names);
    let mut with_path = in_dirs(PATH_DIRS, &names);
    with_path.extend(elsewhere.clone());
    let unix = Platform::unix();

    let (_, attempts, _) = resolve("npx", USER_DIRS, &MockEnv::new(), None, &unix);
    assert_eq!(attempts, recorded(&elsewhere, None));

    let only_path = MockEnv::new().with_var("PATH", PATH);
    let (_, attempts, _) = resolve("npx", USER_DIRS, &only_path, None, &unix);
    assert_eq!(attempts, recorded(&with_path, None));
}

/// Each `PATH` directory is tried under every name before the next, a
/// version manager's directory comes after `PATH` and only for Node's
/// commands, and the user's directories come last.
#[test]
fn on_windows_a_command_is_looked_for_in_one_order_and_the_search_stops_at_the_first_hit() {
    let cases: Vec<(&str, &str, Option<PathBuf>)> = vec![
        (
            "found on PATH, by an extension",
            "tool",
            Some(Path::new(WINDOWS_PATH_DIRS[1]).join("tool.cmd")),
        ),
        (
            "found through a Node version manager",
            "npx",
            Some(Path::new(WINDOWS_NODE_DIR).join("npx.cmd")),
        ),
        (
            "found in the user's last directory",
            "tool",
            Some(Path::new(WINDOWS_USER_DIRS[2]).join("tool.exe")),
        ),
        (
            "found in the user's last directory, past the Node manager",
            "npx",
            Some(Path::new(WINDOWS_USER_DIRS[2]).join("npx.cmd")),
        ),
        ("not found", "tool", None),
        ("not found, a Node command", "npx", None),
    ];

    for (name, command, executable) in cases {
        let executable = executable.as_deref();
        let order = windows_order(command);

        let (resolved, attempts, warnings) = resolve(
            command,
            WINDOWS_USER_DIRS,
            &windows_env(),
            executable,
            &Platform::windows(None),
        );

        assert_eq!(resolved.as_deref(), executable, "{name}: resolved path");
        assert_eq!(attempts, recorded(&order, executable), "{name}: attempts");
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
    }
}

/// An absolute path is tried in its own folder under every name, as
/// written last, before its name is searched for.
#[test]
fn on_windows_an_absolute_path_that_is_gone_is_recorded_then_its_name_is_searched_for() {
    let gone = absolute(&["gone", "npx"]);
    let folder = gone.parent().unwrap().to_str().unwrap();
    let mut order = in_dirs(&[folder], &windows_names("npx"));
    order.extend(windows_order("npx"));

    let executable = Path::new(WINDOWS_USER_DIRS[0]).join("npx.cmd");
    let (resolved, attempts, warnings) = resolve(
        gone.to_str().unwrap(),
        WINDOWS_USER_DIRS,
        &windows_env(),
        Some(&executable),
        &Platform::windows(None),
    );

    assert_eq!(resolved.as_deref(), Some(executable.as_path()));
    assert_eq!(attempts, recorded(&order, Some(&executable)));
    assert_eq!(warnings, [fell_back(&gone, "npx")]);
}

#[test]
fn on_windows_a_path_or_a_version_manager_that_is_not_set_is_not_searched() {
    let names = windows_names("npx");
    let elsewhere = in_dirs(WINDOWS_USER_DIRS, &names);
    let mut with_path = in_dirs(WINDOWS_PATH_DIRS, &names);
    with_path.extend(elsewhere.clone());
    let windows = Platform::windows(None);

    let (_, attempts, _) = resolve("npx", WINDOWS_USER_DIRS, &MockEnv::new(), None, &windows);
    assert_eq!(attempts, recorded(&elsewhere, None));

    let only_path = MockEnv::new().with_var("PATH", WINDOWS_PATH);
    let (_, attempts, _) = resolve("npx", WINDOWS_USER_DIRS, &only_path, None, &windows);
    assert_eq!(attempts, recorded(&with_path, None));
}

// Unix only: the default directories, and on macOS `/etc/paths`, are searched
// only there. Windows' order is the `on_windows_` tests above.
#[cfg(unix)]
mod this_machine {
    use super::*;

    /// This platform's default directories, in the order they are tried.
    const DEFAULT_DIRS: &[&str] = if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
    } else {
        &["/usr/local/bin", "/usr/bin", "/bin"]
    };

    /// What `/etc/paths` and `/etc/paths.d/*` name on this machine. On macOS
    /// the resolver reads them from the real filesystem, so the expectation
    /// has to.
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

    /// The directories this machine adds of its own, in the order they are
    /// tried: what `/etc/paths` names, then the default directories.
    fn machine_dirs() -> Vec<String> {
        let mut dirs = etc_paths_dirs();
        dirs.extend(DEFAULT_DIRS.iter().map(|dir| (*dir).to_string()));
        dirs
    }

    #[test]
    fn its_own_directories_are_tried_after_path_and_before_the_node_managers() {
        let home = home_with_nvm();
        let home = home.path();
        let machine = machine_dirs();
        let [first_default, .., last_default] = DEFAULT_DIRS else {
            panic!("no default directories");
        };

        let cases: Vec<(&str, &str, Option<PathBuf>)> = vec![
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
            ("not found", "tool", None),
            ("not found, a Node command", "npx", None),
        ];

        for (name, command, executable) in cases {
            let executable = executable.as_deref();
            let order = unix_order(command, home, &machine);

            let (resolved, attempts, warnings) = resolve(
                command,
                USER_DIRS,
                &env(home),
                executable,
                &Platform::host(),
            );

            assert_eq!(resolved.as_deref(), executable, "{name}: resolved path");
            assert_eq!(attempts, recorded(&order, executable), "{name}: attempts");
            assert!(warnings.is_empty(), "{name}: {warnings:?}");
        }

        // With no `PATH` and no home, they are searched all the same.
        let names = unix_names("npx");
        let mut elsewhere = in_dirs(&machine, &names);
        elsewhere.extend(in_dirs(USER_DIRS, &names));
        let (_, attempts, _) = resolve("npx", USER_DIRS, &MockEnv::new(), None, &Platform::host());
        assert_eq!(attempts, recorded(&elsewhere, None));
    }
}
