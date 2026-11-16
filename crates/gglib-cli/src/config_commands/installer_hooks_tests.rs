//! The desktop app's Windows installer runs the `gglib.exe` it installs to
//! put that folder on the user's `PATH`, and its uninstaller runs it to take
//! the folder off: `src-tauri/windows/installer-hooks.nsh`. These read that
//! file and the config that hands it to Tauri, and parse each `gglib`
//! command line in it with the CLI's own parser, so the installer cannot
//! name a command this gglib does not have.

use std::path::{Path, PathBuf};

use super::{ConfigCommand, PathEntryCommand};
use crate::commands::Commands;
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

/// The `config path-entry` command `args`, after the program's name, are to
/// the CLI's own parser, or `None` when they are any other command line or
/// none it takes.
fn path_entry(args: &[String]) -> Option<PathEntryCommand> {
    let line = std::iter::once("gglib".to_owned()).chain(args.iter().cloned());
    match parse_from(line).ok()?.0.command {
        Some(Commands::Config {
            command: ConfigCommand::PathEntry { command },
        }) => Some(command),
        _ => None,
    }
}

#[test]
fn path_entry_is_hidden_from_help() {
    use clap::CommandFactory as _;
    let cli = crate::parser::Cli::command();
    let config = cli.find_subcommand("config").expect("gglib has config");
    let entry = config
        .find_subcommand("path-entry")
        .expect("config has path-entry");
    assert!(entry.is_hide_set(), "an installer's command, not a user's");
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

/// The `${If}` branches the line at `at` runs in, outermost first: each the
/// `${If}`, `${ElseIf}` or `${Else}` line that opened its branch.
fn branches<'a>(lines: &[&'a str], at: usize) -> Vec<&'a str> {
    let mut open = Vec::new();
    for line in &lines[..at] {
        if line.starts_with("${If}")
            || line.starts_with("${IfNot}")
            || line.starts_with("${Unless}")
        {
            open.push(*line);
        } else if line.starts_with("${ElseIf}") || line.starts_with("${Else}") {
            open.pop();
            open.push(*line);
        } else if line.starts_with("${EndIf}") {
            open.pop();
        }
    }
    open
}

/// Where the one line that runs `gglib.exe` is in `lines`.
fn gglib_line(lines: &[&str]) -> usize {
    let found: Vec<usize> = (0..lines.len())
        .filter(|at| lines[*at].starts_with("nsExec::") && lines[*at].contains(r"\gglib.exe"))
        .collect();
    assert_eq!(found.len(), 1, "one gglib run in {lines:?}");
    found[0]
}

#[test]
fn the_install_puts_its_folder_on_the_path() {
    let hooks = read(&src_tauri().join("windows/installer-hooks.nsh"));
    let body = macro_body(&hooks, "NSIS_HOOK_POSTINSTALL");
    let runs = gglib_runs(&body);
    assert_eq!(runs, [["config", "path-entry", "add"]]);
    assert_eq!(path_entry(&runs[0]), Some(PathEntryCommand::Add));
    // Every install runs it: an update too, which is still one entry.
    assert_eq!(branches(&body, gglib_line(&body)), Vec::<&str>::new());
}

#[test]
fn the_uninstall_takes_its_folder_off_the_path() {
    // Before the files go: the program that edits the PATH is one of them.
    let hooks = read(&src_tauri().join("windows/installer-hooks.nsh"));
    let body = macro_body(&hooks, "NSIS_HOOK_PREUNINSTALL");
    let runs = gglib_runs(&body);
    assert_eq!(runs, [["config", "path-entry", "remove"]]);
    assert_eq!(path_entry(&runs[0]), Some(PathEntryCommand::Remove));

    // Not for an update, which installs into the same folder at once.
    let remove = gglib_line(&body);
    assert_eq!(branches(&body, remove), ["${If} $UpdateMode <> 1"]);

    // Not before the uninstall can no longer be cancelled: the question
    // whether to close a running app comes first, in the same branch.
    let check = body
        .iter()
        .position(|line| {
            *line == r#"!insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}""#
        })
        .expect("the hook asks whether to close a running app");
    assert!(check < remove, "the question comes before the PATH edit");
    assert_eq!(branches(&body, check), branches(&body, remove));
}

#[test]
fn the_installer_commands_run_before_anything_is_set_up() {
    let command = |args: &[&str]| {
        let Ok((cli, _)) = parse_from(std::iter::once("gglib").chain(args.iter().copied())) else {
            panic!("{args:?} does not parse");
        };
        cli.command
    };
    for action in ["add", "remove"] {
        let parsed = command(&["config", "path-entry", action]);
        assert!(
            crate::dispatch::leaves_nothing_behind(parsed.as_ref()),
            "{action}"
        );
    }
    let paths = command(&["config", "paths"]);
    assert!(!crate::dispatch::leaves_nothing_behind(paths.as_ref()));
    assert!(!crate::dispatch::leaves_nothing_behind(None));
}
