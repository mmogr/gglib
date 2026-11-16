//! Stopping a server started through a shim stops the server behind it
//! (#2012), through the client's own start and stop.
//!
//! The stand-in server behind the wrapper writes its pid, answers
//! `initialize` and sleeps; the wrapper ([`gglib_test_proc::wrap`]) is what
//! the client starts, as it starts `npx.cmd`. After [`McpClient::disconnect`],
//! the process with that pid must be gone within [`GONE`]. A server left
//! running is killed by the test, so none outlives it.

use std::path::Path;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::McpClient;

/// How long the stand-in lives unless something ends it.
const LIFE: Duration = Duration::from_secs(30);
/// How long the test waits for the stand-in to write its pid.
const START: Duration = Duration::from_secs(20);
/// How soon it must be gone after the client stops it.
const GONE: Duration = Duration::from_secs(5);

/// The stand-in's answer to the client's first request, `initialize`.
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","serverInfo":{"name":"stand-in"},"capabilities":{}}}"#;

#[tokio::test]
async fn stopping_a_server_started_through_a_shim_stops_the_server_behind_it() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("server.pid");
    let server = gglib_test_proc::install(
        dir.path(),
        "server",
        &format!(
            "pid {}\nprint {INITIALIZED}\nsleep {}",
            pid_file.display(),
            LIFE.as_secs()
        ),
    );
    let shim = gglib_test_proc::wrap(dir.path(), "server-shim", &server);

    let mut client = McpClient::new();
    client
        .connect_stdio(shim.to_str().unwrap(), &[], None, None, &[])
        .await
        .expect("the client starts the server through its shim");
    let behind = Behind(read_pid(&pid_file).await);
    assert!(running(behind.0), "the server behind the shim is running");

    client.disconnect();

    let deadline = Instant::now() + GONE;
    while running(behind.0) {
        assert!(
            Instant::now() < deadline,
            "the server behind the shim was still running {}s after the client stopped it",
            GONE.as_secs()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The pid the stand-in wrote to `file`, once it has.
async fn read_pid(file: &Path) -> u32 {
    let deadline = Instant::now() + START;
    loop {
        if let Ok(text) = std::fs::read_to_string(file) {
            return text.trim().parse().expect("a pid");
        }
        assert!(Instant::now() < deadline, "the server never wrote its pid");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Whether a process with `pid` is running.
fn running(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(pid)
        .is_some_and(|process| process.status() != sysinfo::ProcessStatus::Zombie)
}

/// The server behind the shim, killed when the test ends however it ends.
struct Behind(u32);

impl Drop for Behind {
    fn drop(&mut self) {
        let mut system = System::new();
        let pid = Pid::from_u32(self.0);
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );
        if let Some(process) = system.process(pid) {
            process.kill();
        }
    }
}
