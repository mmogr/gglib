//! The desktop app's Windows installer runs the `gglib.exe` it installs to
//! put that folder on the user's `PATH`, and its uninstaller runs it to take
//! the folder off: `src-tauri/windows/installer-hooks.nsh`. These read that
//! file and the config that hands it to Tauri, and parse each `gglib`
//! command line in it with the CLI's own parser, so the installer cannot
//! name a command this gglib does not have.

use std::path::{Path, PathBuf};

use crate::remote_flag::parse_from;

/// The desktop app's folder, `src-tauri`.
fn src_tauri() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The lines between `!macro <name>` and its `!macroend`.
fn macro_body<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let mut lines = text.lines().map(str::trim);
    assert!(
        lines.any(|line| line == format!("!macro {name}")),
        "the hooks define no {name}"
    );
    lines.take_while(|line| *line != "!macroend").collect()
}

/// The arguments of each `gglib.exe` the lines run, as the installer passes
/// them: the words after `"$INSTDIR\gglib.exe"`, up to the quote that ends
/// the command.
fn gglib_runs(lines: &[&str]) -> Vec<Vec<String>> {
    const PROGRAM: &str = r#"'"$INSTDIR\gglib.exe""#;
    lines
        .iter()
        .filter(|line| line.starts_with("nsExec::"))
        .filter_map(|line| line.split_once(PROGRAM).map(|(_, rest)| rest))
        .map(|rest| {
            let command = rest.split_once('\'').map_or(rest, |(command, _)| command);
            command.split_whitespace().map(str::to_owned).collect()
        })
        .collect()
}

/// Whether `args`, after the program's name, is a command line the CLI takes.
fn parses(args: &[String]) -> bool {
    parse_from(std::iter::once("gglib".to_owned()).chain(args.iter().cloned())).is_ok()
}

#[test]
fn the_windows_build_hands_the_hooks_to_the_installer() {
    let config: serde_json::Value =
        serde_json::from_str(&read(&src_tauri().join("tauri.windows.conf.json")))
            .expect("tauri.windows.conf.json is JSON");
    let nsis = &config["bundle"]["windows"]["nsis"];
    assert_eq!(nsis["installerHooks"], "windows/installer-hooks.nsh");
    // The hooks edit this user's PATH, which needs no elevation; an install
    // for every user would want the machine's.
    assert_eq!(nsis["installMode"], "currentUser");
}

#[test]
fn the_install_puts_its_folder_on_the_path() {
    let hooks = read(&src_tauri().join("windows/installer-hooks.nsh"));
    let runs = gglib_runs(&macro_body(&hooks, "NSIS_HOOK_POSTINSTALL"));
    assert_eq!(runs, [["config", "path-entry", "add"]]);
    assert!(parses(&runs[0]), "gglib takes {:?}", runs[0]);
}

#[test]
fn the_uninstall_takes_its_folder_off_the_path() {
    // Before the files go: the program that edits the PATH is one of them.
    let hooks = read(&src_tauri().join("windows/installer-hooks.nsh"));
    let runs = gglib_runs(&macro_body(&hooks, "NSIS_HOOK_PREUNINSTALL"));
    assert_eq!(runs, [["config", "path-entry", "remove"]]);
    assert!(parses(&runs[0]), "gglib takes {:?}", runs[0]);
}
