//! End-to-end daemon lifecycle tests against the real `gglib` binary.
//!
//! Ignored by default: the daemon binds the fixed loopback port (9887 by
//! design — see `gglib_core::DAEMON_PORT`), so these tests contend with any
//! daemon already running on the machine. Run them explicitly with
//! `cargo test -p gglib-cli --test daemon_lifecycle -- --ignored` on a
//! machine with no daemon up. Data is isolated via `GGLIB_DATA_DIR`; the
//! port cannot be.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn gglib() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gglib"))
}

fn spawn_daemon(data_dir: &std::path::Path) -> Child {
    let mut cmd = gglib();
    cmd.args(["daemon", "run"])
        .env("GGLIB_DATA_DIR", data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn().expect("spawning gglib daemon run")
}

fn wait_for_health(budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(body) = ureq_get_health()
            && body.contains("gglib-daemon")
        {
            return true;
        }
    }
    false
}

/// Plain-std HTTP GET of /health — no client crate needed for one probe.
fn ureq_get_health() -> std::io::Result<String> {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", gglib_core::DAEMON_PORT))?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

/// The singleton guarantee, end to end: a second `gglib daemon run` refuses
/// to start while the first holds the lock, and names the running daemon.
#[test]
#[ignore = "binds the fixed daemon port; run explicitly on a machine with no daemon up"]
fn a_second_daemon_refuses_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = spawn_daemon(dir.path());
    assert!(
        wait_for_health(Duration::from_secs(15)),
        "daemon never came up"
    );

    let second = gglib()
        .args(["daemon", "run"])
        .env("GGLIB_DATA_DIR", dir.path())
        .output()
        .expect("running second daemon");
    assert!(
        !second.status.success(),
        "second daemon must refuse to start"
    );
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains("already running"),
        "refusal must name the running daemon: {stderr}"
    );

    let _ = first.kill();
    let _ = first.wait();
}

/// SIGTERM produces a clean, bounded exit: the ordered teardown runs (no
/// force-exit), and the lock is released so the next daemon can start.
#[test]
#[ignore = "binds the fixed daemon port; run explicitly on a machine with no daemon up"]
#[cfg(unix)]
fn sigterm_shuts_the_daemon_down_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let mut daemon = spawn_daemon(dir.path());
    assert!(
        wait_for_health(Duration::from_secs(15)),
        "daemon never came up"
    );

    // SIGTERM — what a service manager sends.
    send_sigterm(daemon.id());

    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = daemon.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "daemon did not exit after SIGTERM"
        );
        std::thread::sleep(Duration::from_millis(250));
    };
    assert!(status.success(), "teardown must exit cleanly, got {status}");

    // The lock is free again: a new daemon can start immediately.
    let mut next = spawn_daemon(dir.path());
    assert!(
        wait_for_health(Duration::from_secs(15)),
        "restart after SIGTERM failed — lock not released?"
    );
    let _ = next.kill();
    let _ = next.wait();
}

/// Send SIGTERM via the `kill` binary — no libc dependency for one signal.
#[cfg(unix)]
fn send_sigterm(pid: u32) {
    let _ = Command::new("kill")
        .args(["-15", &pid.to_string()])
        .status();
}
