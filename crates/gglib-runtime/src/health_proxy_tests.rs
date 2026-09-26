//! A health check of a server on this machine goes to that server, never to a
//! proxy somebody set for the internet.
//!
//! Proxy settings are process-wide environment, and this crate denies `unsafe`,
//! which edition 2024 requires to set a variable. So the check runs in a child:
//! the parent starts a recorder, re-runs this binary with the proxy variables
//! pointing at it, and then reads what the recorder saw and what the child
//! found.
//!
//! The recorder must see exactly one request. That one is the child's own
//! positive control, a deliberately proxied request for a name that does not
//! resolve, which can only succeed by going through the recorder: it proves the
//! variables were live in the child, so that an inert environment — a renamed
//! variable, an emptied value — cannot pass the guard by proving nothing. The
//! two health checks must add nothing to that count.
//!
//! The child is chosen by name and by an environment variable this parent sets,
//! and it is deliberately not `#[ignore]`d. An ignored child would have to be
//! run with `--ignored`, and the only thing then keeping the other ignored tests
//! in this binary out of the child run would be the exactness of one filter
//! string — and those tests delete the developer's pidfiles and sweep live
//! servers. Without `--ignored` the worst a wrong filter can do is run ordinary
//! tests twice.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// Printed by the child once both checks reached the port named in the URL.
const MARKER: &str = "LOOPBACK-HEALTH-REACHED-THE-PORT-IN-THE-URL";

/// The child's path in this test binary. A rename leaves the child unrun, and
/// the parent then fails on the missing marker rather than passing quietly.
const CHILD: &str = "health::health_proxy_tests::the_child_checks_health_with_a_proxy_set";

/// Set by the parent for the child, so that a normal `cargo test` run — or a
/// developer whose own environment carries a proxy — leaves the child a no-op.
const CHILD_ENV: &str = "GGLIB_HEALTH_PROXY_CHILD";

/// A name that never resolves, so the positive control can only succeed by
/// being handed to the proxy.
const OFF_BOX: &str = "http://health-check.invalid/health";

/// How long the parent waits for the child to exit.
///
/// Generous on purpose. The child's own work is three requests under a
/// two-second timeout, so this bounds a hang; it does not measure the machine.
const CHILD_BUDGET: Duration = Duration::from_mins(2);

/// How long the parent waits, after the child has exited, for its pipes to
/// reach end of file.
///
/// A pipe stays open past the child's exit only if something the child spawned
/// inherited it and is still running. That is a defect in the child, and it is
/// reported as a missing marker rather than waited out.
const PIPE_GRACE: Duration = Duration::from_secs(5);

/// Read an HTTP request's head, up to and including the blank line.
///
/// Every server here reads the request before answering, and not only for
/// form's sake: a socket closed with unread bytes still in its receive buffer
/// is reset rather than closed, and the answer already on the wire can be lost
/// to the peer along with it.
fn read_request_head(stream: &TcpStream) {
    let Ok(peek) = stream.try_clone() else { return };
    let mut reader = BufReader::new(peek);
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line == "\r\n" || line == "\n" {
            break;
        }
        line.clear();
    }
}

/// Read one HTTP request and answer it the way llama-server answers `/health`.
fn answer_as_a_healthy_server(mut stream: TcpStream) {
    read_request_head(&stream);
    let body = b"{\"status\":\"ok\"}";
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

/// Read a pipe to its end on a thread of its own, and hand the text over a
/// channel so the parent can wait for it with a deadline.
///
/// The parent must drain both of the child's pipes while it runs, not after it
/// exits: a child that fills the pipe buffer blocks in `write` and never
/// reaches its own exit. And the parent must not join the reader without a
/// bound, because a pipe only reaches end of file once every process holding
/// its write end has gone.
fn drain(pipe: Option<impl Read + Send + 'static>) -> Receiver<String> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_string(&mut text);
        }
        let _ = tx.send(text);
    });
    rx
}

#[test]
fn a_health_check_of_this_machine_ignores_a_proxy_in_the_environment() {
    let recorder = TcpListener::bind("127.0.0.1:0").expect("bind a recorder");
    let recorder_port = recorder
        .local_addr()
        .expect("the recorder's address")
        .port();
    let seen = Arc::new(AtomicUsize::new(0));

    let counted = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in recorder.incoming() {
            let Ok(mut stream) = stream else { break };
            counted.fetch_add(1, Ordering::SeqCst);
            // Answer as a proxy that cannot help: the control expects exactly
            // this, and a health check that lands here fails the child's
            // assertion instead of hanging it.
            read_request_head(&stream);
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
        }
    });

    let proxy = format!("http://127.0.0.1:{recorder_port}");
    let mut child = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .env("HTTP_PROXY", &proxy)
        .env("http_proxy", &proxy)
        .env("ALL_PROXY", &proxy)
        .env("all_proxy", &proxy)
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        // A matcher built while this is set ignores every proxy variable, which
        // would make the child pass without proving anything.
        .env_remove("REQUEST_METHOD")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("re-run this binary for the child test");

    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());

    let deadline = Instant::now() + CHILD_BUDGET;
    let status = loop {
        match child.try_wait().expect("wait for the child") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                panic!("the child test did not finish within {CHILD_BUDGET:?}");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let stdout = out.recv_timeout(PIPE_GRACE).unwrap_or_default();
    let stderr = err.recv_timeout(PIPE_GRACE).unwrap_or_default();

    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "the recorder should have seen exactly the child's control request; \
         more means a health check of this machine was posted to the proxy, \
         fewer means the proxy variables were not live in the child:\n{stdout}\n{stderr}"
    );
    assert!(
        status.success(),
        "a health check failed with a proxy in the environment:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains(MARKER),
        "the child never reported reaching its own port:\n{stdout}\n{stderr}"
    );
}

/// Run by the parent above, with the proxy variables set. A run without the
/// parent's variable — an ordinary `cargo test`, or a developer whose shell
/// carries a proxy — does nothing.
#[tokio::test]
async fn the_child_checks_health_with_a_proxy_set() {
    if std::env::var(CHILD_ENV).is_err() {
        println!("not the parent's child run: nothing to check");
        return;
    }
    let proxy = std::env::var("HTTP_PROXY").expect("the parent sets the proxy variables");

    // The positive control: a client built the ordinary way, asked for a name
    // that cannot resolve. It can only get an answer by going through the
    // recorder, whose answer is 502. Anything else means the proxy variables
    // are not live in this process, and the checks below would prove nothing.
    let proxied = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("a client with reqwest's defaults");
    let control = proxied
        .get(OFF_BOX)
        .send()
        .await
        .map(|response| response.status().as_u16())
        .map_err(|e| e.to_string());
    assert_eq!(
        control,
        Ok(502),
        "the control request did not go through {proxy}, so the proxy variables are not live here"
    );

    let server = TcpListener::bind("127.0.0.1:0").expect("bind a health endpoint");
    let port = server.local_addr().expect("the endpoint's address").port();
    // Two connections, because the two checks below use a client each and
    // neither reuses the other's. The server closes each one after answering.
    std::thread::spawn(move || {
        for stream in server.incoming().take(2) {
            let Ok(stream) = stream else { break };
            answer_as_a_healthy_server(stream);
        }
    });

    let monitor_side = super::check_http_health(port).await;
    let fast_path = crate::process::check_http_health(port).await;

    assert!(
        matches!(monitor_side, Ok(true)),
        "the monitor's check did not reach 127.0.0.1:{port} with {proxy} set: {monitor_side:?}"
    );
    assert!(
        fast_path,
        "the fast path's check did not reach 127.0.0.1:{port} with {proxy} set"
    );
    println!("{MARKER}");
}
