//! `gglib daemon stop` with no service running, as the Windows installer
//! runs it before it writes an install's files and the uninstaller before
//! it removes them: it says so, exits 0, and leaves nothing behind. No data
//! folder, no log file and no library are made, so an uninstall does not
//! make again a data folder the user deleted.
//!
//! The built binary runs with `GGLIB_DATA_DIR` naming a folder of the
//! test's own and `GGLIB_DAEMON_PORT` a port nothing listens on, so it
//! never reaches a service running on this machine.

use std::net::TcpListener;
use std::path::Path;

use gglib_core::utils::process::cmd;

/// A loopback port nothing listens on once this returns.
fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a port");
    listener.local_addr().expect("its address").port()
}

/// Run `gglib daemon stop` with its data folder at `data`; its stderr.
fn stop(data: &Path, port: u16) -> String {
    let output = cmd(env!("CARGO_BIN_EXE_gglib"))
        .args(["daemon", "stop"])
        .env("GGLIB_DATA_DIR", data)
        .env("GGLIB_DAEMON_PORT", port.to_string())
        .env("NO_COLOR", "1")
        .output()
        .expect("gglib runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "{stderr}");
    stderr
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
fn a_stop_with_nothing_running_leaves_no_data_folder_and_no_log() {
    let scratch = tempfile::tempdir().expect("a temporary folder");
    let absent = scratch.path().join("data");
    let empty = scratch.path().join("empty");
    std::fs::create_dir(&empty).expect("an empty folder");
    let port = free_port();

    let said = stop(&absent, port);
    assert!(said.contains("Daemon is not running."), "{said}");
    stop(&empty, port);

    assert_eq!(contents(&absent), None, "a data folder was made");
    assert_eq!(contents(&empty), Some(Vec::new()), "something was written");
}
