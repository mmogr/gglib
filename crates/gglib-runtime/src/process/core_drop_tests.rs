//! Tests for the `Drop` of [`super::GuiProcessCore`], the backstop that stops
//! whatever the core still tracks when nothing called `kill` first.
//!
//! Unix-only for the shell script they spawn as llama-server and the `ps`
//! they read its state with, not for the behaviour.

use super::GuiProcessCore;
use crate::pidfile::{PidFileData, delete_pidfile, read_pidfile};
use gglib_core::ports::ServerConfig;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

/// Ids far outside anything a real catalog hands out, for the reason
/// `residency::launch_tests` gives, and used by no other test.
const FIRST_ID: i64 = 999_006;
const SECOND_ID: i64 = 999_007;
const PIDFILE_ID: i64 = 999_008;

/// A core whose llama-server is a script that sleeps for 30 seconds, so a
/// child stays running until something stops it.
///
/// `exec` makes the pid the core records the sleeping process itself, so a
/// kill sent to that pid stops the sleep rather than orphaning it.
fn core_of_sleepers(dir: &Path) -> GuiProcessCore {
    let script = dir.join("llama-server");
    std::fs::write(&script, "#!/bin/sh\nexec sleep 30\n").expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(dir.join("model.gguf"), b"not really a gguf").expect("write model file");
    GuiProcessCore::new(19100, script.to_string_lossy())
}

fn config(dir: &Path, model_id: i64) -> ServerConfig {
    ServerConfig::new(
        model_id,
        "test-model".to_owned(),
        dir.join("model.gguf"),
        19100,
    )
}

/// Whether `pid` is running, as `ps` reports it.
///
/// A zombie counts as stopped: it has exited and waits only to be reaped,
/// which is tokio's job for a child it spawned, so the test does not reap.
fn is_running(pid: u32) -> bool {
    let out = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("run ps");
    let stat = String::from_utf8_lossy(&out.stdout);
    let stat = stat.trim();
    !stat.is_empty() && !stat.starts_with('Z')
}

/// Whether `pid` stops running within five seconds.
async fn stops(pid: u32) -> bool {
    for _ in 0..100 {
        if !is_running(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Leave nothing behind: `Drop` keeps the pidfile, and a failing run can
/// leave the child running. Only a child that did not stop is signalled, so a
/// passing run sends nothing to a pid the OS may since have handed on.
fn clean_up(model_id: i64, pid: u32, stopped: bool) {
    if !stopped {
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
    }
    delete_pidfile(model_id).ok();
}

/// The backstop for a core that goes away with children it still tracks:
/// nothing called `kill`, so `Drop` is all that is left to stop them.
#[tokio::test]
async fn dropping_the_core_stops_every_child_it_tracks() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut core = core_of_sleepers(dir.path());
    let (_, first) = core
        .spawn(config(dir.path(), FIRST_ID))
        .await
        .expect("spawn");
    let (_, second) = core
        .spawn(config(dir.path(), SECOND_ID))
        .await
        .expect("spawn");
    let ran = [is_running(first), is_running(second)];

    drop(core);

    let stopped = [stops(first).await, stops(second).await];
    clean_up(FIRST_ID, first, stopped[0]);
    clean_up(SECOND_ID, second, stopped[1]);
    assert_eq!(ran, [true, true], "both children must run before the drop");
    assert_eq!(
        stopped,
        [true, true],
        "dropping the core must stop every child"
    );
}

/// `Drop` stops the child and leaves its pidfile. The sweep is what deletes a
/// pidfile, after checking whose pid it names, and without the file it could
/// not find a child this backstop failed to stop.
#[tokio::test]
async fn dropping_the_core_leaves_the_pidfile_for_the_sweep() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut core = core_of_sleepers(dir.path());
    let (port, pid) = core
        .spawn(config(dir.path(), PIDFILE_ID))
        .await
        .expect("spawn");

    drop(core);

    let recorded = read_pidfile(PIDFILE_ID).ok();
    clean_up(PIDFILE_ID, pid, stops(pid).await);
    assert_eq!(recorded, Some(PidFileData { pid, port }));
}
