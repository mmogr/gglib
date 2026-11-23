//! End-to-end daemon lifecycle tests against the real `gglib` binary.
//!
//! Each test's daemon runs over a data folder and a daemon port of the
//! test's own (`GGLIB_DATA_DIR`, `GGLIB_DAEMON_PORT`), with its endpoint
//! kept off, and the test asks only that port for `/health`. So no test
//! reaches another test's daemon or one already running on the machine.

#[path = "support/data_dir.rs"]
mod data_dir;

use gglib_core::utils::process::cmd;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a daemon has to answer `/health`, and to exit once stopped.
const BUDGET: Duration = Duration::from_secs(15);

fn gglib() -> Command {
    cmd(env!("CARGO_BIN_EXE_gglib"))
}

/// A data folder with its endpoint kept off
/// ([`data_dir::keep_the_endpoint_off`]), and a port nothing answered on
/// when it was picked, for this test's daemons.
struct Place {
    dir: tempfile::TempDir,
    port: u16,
}

impl Place {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a data folder");
        data_dir::keep_the_endpoint_off(dir.path());
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("a free port")
            .port();
        Self { dir, port }
    }

    fn data(&self) -> &Path {
        self.dir.path()
    }

    /// `command`, a command for the `gglib` binary, as `gglib daemon run`
    /// over this data folder and port.
    fn daemon_run(&self, mut command: Command) -> Command {
        command
            .args(["daemon", "run"])
            .env("GGLIB_DATA_DIR", self.data())
            .env("GGLIB_DAEMON_PORT", self.port.to_string())
            .stdin(Stdio::null());
        command
    }

    /// Start `gglib daemon run` here and wait until it answers `/health`.
    fn start(&self) -> Daemon {
        let mut daemon = Daemon::start(self.daemon_run(gglib()));
        daemon.wait_until_up(self.port);
        daemon
    }
}

/// A running `gglib daemon run`, its stderr read on a thread of its own so
/// the pipe never fills. Dropping it kills the daemon, so a failed test
/// leaves none running.
struct Daemon {
    child: Child,
    stderr: Option<JoinHandle<String>>,
}

impl Daemon {
    fn start(mut command: Command) -> Self {
        let mut child = command
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("gglib daemon run starts");
        let mut pipe = child.stderr.take().expect("its stderr");
        let stderr = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        });
        Self {
            child,
            stderr: Some(stderr),
        }
    }

    /// Wait until the daemon answers `/health` on `port`, as a daemon.
    ///
    /// # Panics
    ///
    /// When it exits first, or does not answer within [`BUDGET`], with what
    /// it wrote to stderr.
    fn wait_until_up(&mut self, port: u16) {
        let deadline = Instant::now() + BUDGET;
        loop {
            if let Some(status) = self.child.try_wait().expect("its status") {
                panic!("the daemon exited with {status}: {}", self.stderr());
            }
            if health(port).is_ok_and(|body| body.contains("gglib-daemon")) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the daemon never answered on {port}: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Wait until the daemon exits, after `asked`.
    ///
    /// # Panics
    ///
    /// When it has not exited within [`BUDGET`], with what it wrote to
    /// stderr.
    fn wait_for_exit(&mut self, asked: &str) -> ExitStatus {
        let deadline = Instant::now() + BUDGET;
        loop {
            if let Some(status) = self.child.try_wait().expect("its status") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "the daemon did not exit after {asked}: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// What the daemon wrote to stderr, once it has exited, killing it first
    /// if it has not. The daemon starts no other program here, so its
    /// stderr closes when it exits.
    fn stderr(&mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stderr
            .take()
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Plain-std HTTP GET of /health on `port`, no client crate needed for one
/// probe.
fn health(port: u16) -> std::io::Result<String> {
    use std::io::Write;
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

/// The singleton guarantee, end to end: a second `gglib daemon run` refuses
/// to start while the first holds the lock, and names the running daemon.
#[test]
fn a_second_daemon_refuses_to_start() {
    let place = Place::new();
    let _first = place.start();

    let second = place
        .daemon_run(gglib())
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
}

/// SIGTERM produces a clean, bounded exit: the ordered teardown runs (no
/// force-exit), and the lock is released so the next daemon can start.
// Unix only: SIGTERM is a signal.
#[cfg(unix)]
#[test]
fn sigterm_shuts_the_daemon_down_cleanly() {
    let place = Place::new();
    let mut daemon = place.start();

    // SIGTERM — what a service manager sends.
    send_sigterm(daemon.child.id());

    let status = daemon.wait_for_exit("SIGTERM");
    assert!(
        status.success(),
        "teardown must exit cleanly, got {status}: {}",
        daemon.stderr()
    );

    // The lock is free again: a new daemon can start immediately.
    let _next = place.start();
}

/// Send SIGTERM via the `kill` binary — no libc dependency for one signal.
#[cfg(unix)]
fn send_sigterm(pid: u32) {
    let _ = cmd("kill").args(["-15", &pid.to_string()]).status();
}

/// What every test that starts a real service stores first: the endpoint
/// off, so the service binds no endpoint port of its own.
#[test]
fn a_test_service_is_started_with_its_endpoint_off() {
    let dir = tempfile::tempdir().unwrap();
    data_dir::keep_the_endpoint_off(dir.path());
    let stored = data_dir::read_settings(dir.path());
    assert_eq!(stored.proxy_autostart, Some(false));
    assert!(!stored.effective_proxy_autostart());
}
