//! Running a program that is expected to answer and exit, with a time limit.
//!
//! `Command::output()` waits for as long as the program runs, and for as long
//! as anything holds its output open. A probe such as `llama-server
//! --version` that never exits, or leaves a process holding its stdout, kept
//! `gglib config llama validate`, the setup status and the commands that ask
//! it waiting with no end (#1984). [`run_bounded`] gives it `wait`, then
//! kills it and says so.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// How long a program asked what it is (`--version`) has to answer. Long
/// enough for a first run that an antivirus scans; far shorter than a hang.
pub const PROBE_WAIT: Duration = Duration::from_secs(10);

/// How often a running program is checked on.
const POLL: Duration = Duration::from_millis(10);

/// Run `command` with nothing on stdin and its output read, and give it
/// `wait` to exit.
///
/// Its stdout and stderr are what it printed. A program still running when
/// `wait` is up is killed, and the answer is an error of kind
/// [`std::io::ErrorKind::TimedOut`]. Output that something else keeps open
/// after the program has exited is read until `wait` is up, and no longer.
///
/// # Errors
///
/// The program could not be started, or it did not exit within `wait`.
pub fn run_bounded(command: &mut Command, wait: Duration) -> std::io::Result<Output> {
    let deadline = Instant::now() + wait;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = read_on_a_thread(child.stdout.take());
    let stderr = read_on_a_thread(child.stderr.take());

    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("did not answer within {} s", wait.as_secs_f32()),
            ));
        }
        thread::sleep(POLL);
    };

    // A moment past the deadline at least: a program that exits just in time
    // has its output read.
    let left = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(100));
    Ok(Output {
        status,
        stdout: stdout.recv_timeout(left).unwrap_or_default(),
        stderr: stderr.recv_timeout(left).unwrap_or_default(),
    })
}

/// Read `stream` to its end on a thread of its own, so a stream something
/// keeps open cannot hold the caller.
fn read_on_a_thread(stream: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (sent, read) = mpsc::channel();
    if let Some(mut stream) = stream {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.read_to_end(&mut bytes);
            let _ = sent.send(bytes);
        });
    } else {
        let _ = sent.send(Vec::new());
    }
    read
}

#[cfg(test)]
#[path = "bounded_tests.rs"]
mod tests;
