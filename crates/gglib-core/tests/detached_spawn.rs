//! A command whose output is captured, and which starts the service, must
//! not hold that output open for as long as the service runs (#1982).
//!
//! On Windows the service inherited every handle the starting command could
//! pass on, the captured output's pipe among them, so `gglib up -y` under a
//! caller that read its output hung until the service and its model process
//! exited.
//!
//! The test runs this test binary three times, each in one part, chosen by
//! [`ROLE`]: the caller reads the starter's output, the starter prepares its
//! process the way `main` does and starts the service, and the service lives
//! far longer than the caller waits.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gglib_core::utils::process::{cmd, detached, host};

/// Which part this run of the test binary plays.
const ROLE: &str = "GGLIB_DETACHED_SPAWN_ROLE";
/// The directory the three parts share.
const DIR: &str = "GGLIB_DETACHED_SPAWN_DIR";
/// This test's name, to run only it.
const NAME: &str = "a_captured_command_that_starts_the_service_ends_before_the_service_does";

/// How long the stand-in service lives, unless the test kills it first.
const SERVICE_LIFE: Duration = Duration::from_mins(1);
/// How long the caller waits for the starter's output to end.
const WAIT: Duration = Duration::from_secs(20);
/// How long the starter waits for the service to say it is running.
const START: Duration = Duration::from_secs(10);

#[test]
fn a_captured_command_that_starts_the_service_ends_before_the_service_does() {
    match std::env::var(ROLE).as_deref() {
        Ok("starter") => starter(),
        Ok("service") => service(),
        _ => caller(),
    }
}

/// The test reading a gglib command's output, as a script or an IDE would.
fn caller() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut service = Service(Some(dir.path().join("service.pid")));

    let mut starter = this_test("starter");
    starter
        .env(DIR, dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut starter = starter.spawn().expect("running the starter");

    let mut output = starter.stdout.take().expect("the starter's output");
    let (sent, ended) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = output.read_to_string(&mut text);
        let _ = sent.send(text);
    });

    let text = ended.recv_timeout(WAIT).unwrap_or_else(|_| {
        panic!(
            "the starter's output was still open {}s after it ran: the service holds the \
             caller's pipe",
            WAIT.as_secs()
        )
    });
    let status = starter.wait().expect("the starter's exit");

    assert!(
        text.contains("service started"),
        "the starter did not start the service ({status}); it printed:\n{text}"
    );
    assert!(status.success(), "the starter failed ({status}):\n{text}");
    assert!(
        service.stop(),
        "the service was gone before the caller's read ended, so the read ending proves nothing"
    );
}

/// The gglib command: prepares its process as `main` does, starts the
/// service, waits until the service is running, and exits.
fn starter() {
    host::prepare();
    let dir = shared_dir();
    let log = File::create(dir.join("service.log")).expect("the service's log");
    detached::spawn_daemon(this_test("service"), &log).expect("starting the service");

    let pid_file = dir.join("service.pid");
    let deadline = Instant::now() + START;
    while !pid_file.exists() {
        assert!(Instant::now() < deadline, "the service never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("service started");
    std::process::exit(0);
}

/// The service: records its pid, then lives far longer than the caller waits.
fn service() {
    let dir = shared_dir();
    let written = dir.join("service.pid.part");
    std::fs::write(&written, std::process::id().to_string()).expect("writing the pid");
    std::fs::rename(&written, dir.join("service.pid")).expect("publishing the pid");
    std::thread::sleep(SERVICE_LIFE);
}

fn shared_dir() -> PathBuf {
    PathBuf::from(std::env::var_os(DIR).expect("the shared directory"))
}

/// This test binary, running only this test, in `role`.
fn this_test(role: &str) -> Command {
    let mut command = cmd(std::env::current_exe().expect("this test binary"));
    command
        .args([NAME, "--exact", "--nocapture"])
        .env(ROLE, role);
    command
}

/// The service the starter started, killed when the test ends however it
/// ends, so no process outlives the test.
struct Service(Option<PathBuf>);

impl Service {
    /// Kill the service; whether it was still running.
    fn stop(&mut self) -> bool {
        self.0.take().is_some_and(|pid_file| kill(&pid_file))
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Kill the process whose pid `pid_file` holds; whether one was killed.
fn kill(pid_file: &Path) -> bool {
    let Ok(pid) = std::fs::read_to_string(pid_file) else {
        return false;
    };
    let mut kill = if cfg!(windows) {
        let mut taskkill = cmd("taskkill");
        taskkill.args(["/F", "/PID", pid.trim()]);
        taskkill
    } else {
        let mut kill = cmd("kill");
        kill.args(["-9", pid.trim()]);
        kill
    };
    kill.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
