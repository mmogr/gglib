//! `gglib config path-entry`, the Windows installer's command, leaves
//! nothing behind but the `PATH` entry it is for: no data folder and no log
//! file, so an uninstall does not make again a data folder the user deleted.
//!
//! The built binary runs with `GGLIB_DATA_DIR` naming a folder of the
//! test's own. `remove` runs on every OS: with no record naming the folder
//! it writes nothing, on Windows too. `add` runs where `remove` says this OS
//! keeps no `PATH` of the user's own, so no test adds a test binary's folder
//! to a real user's `PATH`.

use std::path::Path;

use gglib_core::utils::process::cmd;

/// Run `gglib config path-entry <action>` with its data folder at `data`;
/// its standard output.
fn path_entry(action: &str, data: &Path) -> String {
    let output = cmd(env!("CARGO_BIN_EXE_gglib"))
        .args(["config", "path-entry", action])
        .env("GGLIB_DATA_DIR", data)
        .output()
        .expect("gglib runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{action}: {stderr}");
    assert!(stderr.is_empty(), "{action} wrote to stderr: {stderr}");
    String::from_utf8(output.stdout).expect("UTF-8")
}

/// What is in `folder`, or `None` when it is not there.
fn contents(folder: &Path) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(folder).ok()?;
    Some(
        entries
            .map(|entry| entry.expect("an entry").path().display().to_string())
            .collect(),
    )
}

#[test]
fn the_installer_commands_leave_no_data_folder_and_no_log() {
    let scratch = tempfile::tempdir().expect("a temporary folder");
    let absent = scratch.path().join("data");
    let empty = scratch.path().join("empty");
    std::fs::create_dir(&empty).expect("an empty folder");

    let said = path_entry("remove", &absent);
    path_entry("remove", &empty);
    if said.starts_with("This system keeps no PATH") {
        path_entry("add", &absent);
        path_entry("add", &empty);
    }

    assert_eq!(contents(&absent), None, "a data folder was made");
    assert_eq!(contents(&empty), Some(Vec::new()), "something was written");
}
