//! The children the service ties to itself end when the service is killed,
//! on the OS that promises it (#1985).
//!
//! On Windows the engines outlived a service that died without its teardown
//! and held their memory until the next start. [`tied::tie_to_this_process`]
//! says what it promises on this OS, and the test holds it to that:
//! [`Tied::ByJob`] means the children are gone within [`GONE`] of the
//! service being killed outright; [`Tied::NotTied`] (macOS and Linux, by
//! decision) means they are still running, and the test stops them itself.
//!
//! The test runs this test binary again as the service, chosen by [`ROLE`].
//! The service starts [`ENGINES`] stand-in engines that print [`READY`] and
//! sleep, with the service's own output, a pipe the test reads. The pipe
//! ends only when every process holding it has ended, so once the service is
//! killed, the end of the output is the last engine ending.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use gglib_core::utils::process::tied::{self, Tied};
use gglib_core::utils::process::{async_cmd, cmd};

/// Which part this run of the test binary plays.
const ROLE: &str = "GGLIB_TIED_CHILD_ROLE";
/// The stand-in engine the service starts.
const ENGINE: &str = "GGLIB_TIED_CHILD_ENGINE";
/// This test's name, to run only it.
const NAME: &str = "a_tied_child_ends_when_the_process_that_tied_it_is_killed";

/// How many engines the service starts: two, so the second is tied to a
/// job that already holds one, as a second model or an MCP server is.
const ENGINES: usize = 2;
/// What a stand-in engine prints once it runs.
const READY: &str = "engine ready";
/// How long a stand-in engine lives unless something ends it.
const ENGINE_LIFE: Duration = Duration::from_mins(1);
/// How long the test waits for the service to start its engines.
const START: Duration = Duration::from_secs(20);
/// How soon the tied engines must be gone after the service is killed.
const GONE: Duration = Duration::from_secs(2);
/// How long untied engines must still be running after the service is
/// killed, for the test to believe nothing tied them.
const STILL: Duration = Duration::from_millis(500);

#[test]
fn a_tied_child_ends_when_the_process_that_tied_it_is_killed() {
    match std::env::var(ROLE).as_deref() {
        Ok("service") => service(),
        _ => test(),
    }
}

/// The test: starts the service, waits for its engines, kills the service
/// outright, and reads whether the engines ended with it.
fn test() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let engine = gglib_test_proc::install(
        dir.path(),
        "engine",
        &format!("print {READY}\nsleep {}", ENGINE_LIFE.as_secs()),
    );

    let mut service = this_test("service");
    service
        .env(ENGINE, &engine)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut service = Service(service.spawn().expect("starting the service"));
    let heard = lines(service.0.stdout.take().expect("the service's output"));

    let mut engines = Engines(Vec::new());
    let mut tied = Vec::new();
    let mut ready = 0;
    let deadline = Instant::now() + START;
    while tied.len() < ENGINES || ready < ENGINES {
        let line = heard
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_else(|_| {
                panic!(
                    "the service had not got {ENGINES} engines running {}s after it started",
                    START.as_secs()
                )
            })
            .expect("the service's output ended before its engines were running");
        if line == READY {
            ready += 1;
        } else if let Some(pid) = line.strip_prefix("engine pid ") {
            engines.0.push(pid.to_owned());
        } else if let Some(what) = line.strip_prefix("tied ") {
            tied.push(match what {
                "ByJob" => Tied::ByJob,
                "NotTied" => Tied::NotTied,
                failed => panic!(
                    "the service could not tie engine {}: {failed}",
                    tied.len() + 1
                ),
            });
        }
    }
    assert!(
        tied.iter().all(|t| *t == tied[0]),
        "the engines were not tied alike: {tied:?}"
    );

    // Killed outright, as `taskkill /F` does on Windows (`TerminateProcess`)
    // and `kill -9` does on macOS and Linux: no teardown runs.
    service.kill();

    match tied[0] {
        Tied::ByJob => {
            assert!(
                ended_within(&heard, GONE),
                "an engine was still running {}s after the service that tied it was killed",
                GONE.as_secs()
            );
            // Gone, so their pids are no longer the engines' to kill.
            engines.0.clear();
        }
        Tied::NotTied => {
            assert!(
                !ended_within(&heard, STILL),
                "the engines ended with the service, though the service said nothing tied them"
            );
            assert!(engines.kill(), "the engines could not be killed");
            assert!(
                ended_within(&heard, GONE),
                "the engines were killed and the output did not end, so the test cannot see \
                 an engine end"
            );
        }
    }
}

/// The service: starts each stand-in engine with its own output, ties it,
/// says what tied it, and lives until the test kills it.
fn service() {
    let engine = std::env::var_os(ENGINE).expect("the stand-in engine");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime to start the engines in");
    let _entered = runtime.enter();

    let mut out = std::io::stdout();
    let mut children = Vec::new();
    for _ in 0..ENGINES {
        let mut command = async_cmd(&engine);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null());
        let child = command.spawn().expect("starting an engine");

        let pid = child.id().expect("the engine's pid");
        writeln!(out, "engine pid {pid}").expect("saying the engine's pid");
        match tied::tie_to_this_process(&child) {
            Ok(tied) => writeln!(out, "tied {tied:?}"),
            Err(e) => writeln!(out, "tied failed: {e}"),
        }
        .expect("saying how the engine is tied");
        out.flush().expect("flushing the output");
        children.push(child);
    }

    std::thread::sleep(ENGINE_LIFE);
    drop(children);
}

/// Each line `output` gives, then `None` when it ends.
fn lines(output: impl std::io::Read + Send + 'static) -> Receiver<Option<String>> {
    let (sent, heard) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            if sent.send(Some(line)).is_err() {
                return;
            }
        }
        let _ = sent.send(None);
    });
    heard
}

/// Whether the output ends within `wait`, passing over any lines first.
fn ended_within(heard: &Receiver<Option<String>>, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        match heard.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Some(_)) => {}
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}

/// This test binary, running only this test, in `role`.
fn this_test(role: &str) -> Command {
    let mut command = cmd(std::env::current_exe().expect("this test binary"));
    command
        .args([NAME, "--exact", "--nocapture"])
        .env(ROLE, role);
    command
}

/// The service, killed when the test ends however it ends.
struct Service(Child);

impl Service {
    fn kill(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.kill();
    }
}

/// The stand-in engines' pids, as the service says them; killed when the
/// test ends however it ends, so no process outlives the test.
struct Engines(Vec<String>);

impl Engines {
    /// Kill every engine; whether there were some and each was killed.
    fn kill(&mut self) -> bool {
        let pids = std::mem::take(&mut self.0);
        let killed = pids.iter().filter(|pid| kill(pid)).count();
        !pids.is_empty() && killed == pids.len()
    }
}

impl Drop for Engines {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Kill the process `pid`; whether it was killed.
fn kill(pid: &str) -> bool {
    let mut kill = if cfg!(windows) {
        let mut taskkill = cmd("taskkill");
        taskkill.args(["/F", "/PID", pid]);
        taskkill
    } else {
        let mut kill = cmd("kill");
        kill.args(["-9", pid]);
        kill
    };
    kill.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
