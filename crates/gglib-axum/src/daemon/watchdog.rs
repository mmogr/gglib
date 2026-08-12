//! Liveness watchdog: a thread that exits a daemon that has stopped answering.
//!
//! The failure this guards against is the worst one the daemon has: wedged but
//! still listening. A deadlocked runtime (issue #721) keeps its LISTEN sockets
//! — TCP connects succeed, so nothing looks dead from outside — while no
//! request is ever serviced. That state blocks graceful `daemon stop` (the
//! stop endpoint is HTTP), blocks a replacement daemon (the port is held), and
//! defeats `ensure_daemon` (a connect is its health signal). The only way out
//! was a manual `kill -9`.
//!
//! The watchdog turns that terminal state into a restart: it probes the
//! daemon's own `/health` from outside the runtime, and after
//! [`FAILURE_THRESHOLD`] consecutive failures it exits the process. Exiting
//! frees the ports and the lock, so the next CLI call's `ensure_daemon` starts
//! a fresh daemon; llama-server children left behind are caught by that
//! start's orphan sweep.
//!
//! Everything here is deliberately runtime-free: a plain OS thread, a blocking
//! `TcpStream`, a hand-written HTTP/1.1 request. The condition being detected
//! is "the async runtime no longer runs anything", so no part of the detector
//! may depend on the runtime — a tokio task or a reqwest client would be
//! starved by exactly the wedge it exists to notice.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use tracing::{error, info, warn};

/// How long the daemon must sit between probes.
///
/// Long enough that the watchdog is invisible in the logs and the load, short
/// enough that a wedged daemon self-clears in under a minute.
const PROBE_INTERVAL: Duration = Duration::from_secs(10);

/// Budget for one probe: connect, write, and read the status line.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Consecutive failed probes before the daemon is declared wedged.
///
/// Three, so one slow moment — a paging storm while a model loads, a laptop
/// waking from sleep mid-probe — never kills a healthy daemon. Tripping
/// requires ~45s of continuous unresponsiveness, and a healthy daemon answers
/// `/health` in microseconds no matter how busy the models are.
const FAILURE_THRESHOLD: u32 = 3;

/// Exit code for a self-detected wedge: `EX_SOFTWARE`, and distinct from the
/// shutdown watchdog's `1`, so `daemon.log` and a supervisor can tell "hung
/// and gave up" from "teardown overran".
const WEDGED_EXIT_CODE: i32 = 70;

/// Start the watchdog thread for a daemon bound to `bound`.
///
/// `GGLIB_DAEMON_WATCHDOG=off` disables it, for debugging sessions where a
/// stopped-in-a-debugger daemon must not be shot for failing probes.
pub(super) fn spawn(bound: SocketAddr) {
    if std::env::var("GGLIB_DAEMON_WATCHDOG").is_ok_and(|v| v.eq_ignore_ascii_case("off")) {
        info!("liveness watchdog disabled by GGLIB_DAEMON_WATCHDOG=off");
        return;
    }

    let target = probe_target(bound);
    let spawned = std::thread::Builder::new()
        .name("gglib-liveness".into())
        .spawn(move || watch(target));
    if let Err(e) = spawned {
        // Degraded but not fatal: the daemon merely loses self-defence.
        warn!("could not start the liveness watchdog thread: {e}");
    }
}

/// The address a probe should dial.
///
/// A daemon bound to the unspecified address (`0.0.0.0`) is reachable on
/// loopback but not *at* `0.0.0.0`; any other bind — loopback or a LAN IP —
/// is dialled exactly where it listens.
fn probe_target(bound: SocketAddr) -> SocketAddr {
    if bound.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bound.port())
    } else {
        bound
    }
}

/// Probe forever; exit the process when the failure streak crosses the line.
fn watch(target: SocketAddr) -> ! {
    let mut streak = FailureStreak::new(FAILURE_THRESHOLD);
    loop {
        std::thread::sleep(PROBE_INTERVAL);
        let healthy = probe(target, PROBE_TIMEOUT);
        if healthy {
            streak.record_success();
            continue;
        }
        warn!(
            failures = streak.failures + 1,
            threshold = FAILURE_THRESHOLD,
            "daemon liveness probe got no answer from /health"
        );
        if streak.record_failure() {
            error!(
                "daemon unresponsive for {FAILURE_THRESHOLD} consecutive probes — exiting \
                 (code {WEDGED_EXIT_CODE}) so the ports free and the next gglib command can \
                 start a fresh daemon; llama-server children are caught by its orphan sweep"
            );
            std::process::exit(WEDGED_EXIT_CODE);
        }
    }
}

/// Consecutive-failure accounting, separated so the tripwire is testable
/// without a clock or a socket.
struct FailureStreak {
    failures: u32,
    threshold: u32,
}

impl FailureStreak {
    const fn new(threshold: u32) -> Self {
        Self {
            failures: 0,
            threshold,
        }
    }

    fn record_success(&mut self) {
        self.failures = 0;
    }

    /// Record one failed probe; `true` means the threshold is reached.
    fn record_failure(&mut self) -> bool {
        self.failures += 1;
        self.failures >= self.threshold
    }
}

/// One round trip: `GET /health`, expect a 200 status line within `timeout`.
///
/// A refused connect counts as a failure just like a hung read: a daemon whose
/// listener is gone but whose process survives is the zombie variant of the
/// same wedge. Anything that answers 200 is alive — the body is not read.
fn probe(target: SocketAddr, timeout: Duration) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&target, timeout) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream.set_write_timeout(Some(timeout)).is_err()
    {
        return false;
    }

    // `Host` must name where we dialled or the daemon's Host guard rejects the
    // probe — loopback is always allowed, and a LAN bind allows its own IP.
    let request = format!("GET /health HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    read_status_line_is_200(&mut stream)
}

/// Read just enough of the response to judge the status line.
fn read_status_line_is_200(stream: &mut impl Read) -> bool {
    const EXPECTED: &[u8] = b"HTTP/1.1 200";
    let mut buf = [0u8; EXPECTED.len()];
    let mut filled = 0;
    while filled < buf.len() {
        match stream.read(&mut buf[filled..]) {
            Ok(0) | Err(_) => return false,
            Ok(n) => filled += n,
        }
    }
    buf == EXPECTED
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A scratch server that answers one connection with `response`.
    ///
    /// Returns only once the serving thread is scheduled and about to
    /// `accept`. Without that handshake the helper returned as soon as the
    /// thread was *spawned*, and the caller could finish connecting, writing
    /// and waiting out its read timeout before the thread ever ran — which
    /// made these tests fail under parallel load while passing in isolation.
    /// The connection itself needs no synchronising: it waits in the
    /// listener's backlog from the moment the socket is bound.
    fn one_shot_server(response: &'static [u8]) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
        let addr = listener.local_addr().expect("bound addr");
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = ready_tx.send(());
            if let Ok((mut stream, _)) = listener.accept() {
                let mut discard = [0u8; 512];
                let _ = stream.read(&mut discard);
                let _ = stream.write_all(response);
            }
        });
        ready_rx.recv().expect("serving thread started");
        addr
    }

    #[test]
    fn a_healthy_daemon_passes_the_probe() {
        let addr = one_shot_server(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok");
        assert!(probe(addr, Duration::from_secs(5)));
    }

    #[test]
    fn a_non_200_answer_fails_the_probe() {
        let addr = one_shot_server(b"HTTP/1.1 403 Forbidden\r\n\r\n");
        assert!(!probe(addr, Duration::from_secs(5)));
    }

    #[test]
    fn a_dead_port_fails_the_probe() {
        // Bind-then-drop yields a port that is closed at probe time.
        let addr = TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral port")
            .local_addr()
            .expect("bound addr");
        assert!(!probe(addr, Duration::from_millis(500)));
    }

    #[test]
    fn a_wedged_daemon_accepts_but_never_answers_and_fails_the_probe() {
        // The #721 signature: the listener accepts, nothing ever responds.
        let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
        let addr = listener.local_addr().expect("bound addr");
        std::thread::spawn(move || {
            let held = listener.accept();
            std::thread::sleep(Duration::from_secs(2));
            drop(held);
        });
        assert!(!probe(addr, Duration::from_millis(300)));
    }

    #[test]
    fn the_streak_trips_only_on_consecutive_failures() {
        let mut streak = FailureStreak::new(3);
        assert!(!streak.record_failure());
        assert!(!streak.record_failure());
        streak.record_success(); // a good probe resets the count
        assert!(!streak.record_failure());
        assert!(!streak.record_failure());
        assert!(streak.record_failure(), "third consecutive failure trips");
    }

    #[test]
    fn an_unspecified_bind_is_probed_on_loopback() {
        let target = probe_target("0.0.0.0:9887".parse().expect("addr"));
        assert_eq!(target, "127.0.0.1:9887".parse().expect("addr"));
        let lan = probe_target("192.168.1.5:9887".parse().expect("addr"));
        assert_eq!(lan, "192.168.1.5:9887".parse().expect("addr"));
    }
}
