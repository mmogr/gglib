//! A stand-in that hears no stop of its own is ended by the one
//! [`ask_to_stop`] sends, long before it would exit by itself.

use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::{ask_to_stop, stoppable};

/// How soon the stand-in must be gone once asked.
const GONE: Duration = Duration::from_secs(10);

#[test]
fn a_child_asked_to_stop_is_sent_the_stop_of_its_os() {
    let dir = tempfile::tempdir().unwrap();
    let program = gglib_test_proc::install(dir.path(), "service", "print started\nsleep 60");
    let mut child = stoppable(&program)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the stand-in starts");

    // Asked only once it runs, so the stop reaches the program itself.
    let mut started = String::new();
    BufReader::new(child.stdout.take().expect("its stdout"))
        .read_line(&mut started)
        .expect("its first line");
    assert_eq!(started.trim(), "started");

    ask_to_stop(&child).expect("the stop is sent");

    let deadline = Instant::now() + GONE;
    let status = loop {
        if let Some(status) = child.try_wait().expect("its status") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the stand-in was still running {GONE:?} after the stop was sent");
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    // Nothing in the stand-in hears the stop, so the OS ends it.
    assert!(!status.success(), "{status}");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        assert_eq!(status.signal(), Some(15), "ended by SIGTERM: {status}");
    }
}
