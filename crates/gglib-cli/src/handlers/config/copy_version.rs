//! Another gglib's version, from its `--version`, for
//! [`copies`](super::copies).
//!
//! Every release, from 0.1.0 on, parses its command line before it opens
//! its library or starts anything, so clap answers `--version` first and
//! asking is safe. A copy that hangs is
//! killed when its wait is up; one that fails, or prints something that is
//! not a version, has none.

use std::io::Read as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long another copy has to answer `--version`.
pub(crate) const VERSION_WAIT: Duration = Duration::from_secs(2);

/// Run `bin --version`, with nothing on stdin, and give it `wait` to answer.
/// A copy that does not start, exits with an error, says nothing that reads
/// as a version, or is still running when `wait` is up (it is then killed)
/// has no version.
pub(crate) fn read_version(bin: &Path, wait: Duration) -> Option<String> {
    let deadline = Instant::now() + wait;
    let mut child = Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    // Read on a thread, so a child that keeps its stdout open (or leaves a
    // process holding it) cannot keep this waiting past `wait`.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.take(4096).read_to_end(&mut out);
        let _ = sender.send(out);
    });
    let out = receiver.recv_timeout(wait).ok();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    if !status?.success() {
        return None;
    }
    version_word(String::from_utf8_lossy(&out?).lines().next()?)
}

/// The version in a line such as `gglib 0.35.0 (a1b2c3d4e5f6)`: its first
/// word that starts with a digit, if that word is short and plain.
pub(super) fn version_word(line: &str) -> Option<String> {
    let word = line
        .split_whitespace()
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))?;
    let plain = word.len() <= 40
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'));
    plain.then(|| word.to_owned())
}
