//! A connection that goes quiet, a resume that checks a large part, and a
//! reconnect that must not splice two files.
//!
//! The quiet time is injected short (`QUIET`): production sets it from
//! [`STALL_AFTER`], and the suite does not wait tens of seconds for it. It
//! is a whole second, against stand-ins that hang for good, so a slow runner
//! that takes a while to connect is not taken for a quiet connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt as _;

use super::tests::{Fixture, Reply, TestServer, body_of, sha256_of};
use super::*;
use crate::executor::to_download_error;

/// The quiet time the tests run with.
const QUIET: Duration = Duration::from_secs(1);

/// How long a test may take before it counts as hung. A download that waits
/// on a quiet connection for ever fails here rather than hanging the suite.
const HUNG: Duration = Duration::from_secs(30);

/// Reconnects after a quiet connection before the download gives up.
const RECONNECTS: usize = STALL_RECONNECTS as usize;

/// What one download reported: every reading and every note.
#[derive(Default)]
struct Seen {
    readings: Mutex<Vec<RawProgress>>,
    notices: Mutex<Vec<String>>,
}

impl Seen {
    fn notices(&self) -> Vec<String> {
        self.notices.lock().unwrap().clone()
    }
}

fn quiet_request<'a>(
    url: &'a str,
    fx: &'a Fixture,
    size: usize,
    seen: &Arc<Seen>,
) -> NativeDownload<'a> {
    let readings = Arc::clone(seen);
    let notices = Arc::clone(seen);
    NativeDownload {
        url,
        dest: &fx.dest,
        commit: None,
        token: None,
        expected_size: Some(size as u64),
        progress: Some(Arc::new(move |raw| {
            readings.readings.lock().unwrap().push(raw);
        })),
        cancel: None,
        notice: Some(Arc::new(move |note: &str| {
            notices.notices.lock().unwrap().push(note.to_string());
        })),
        stall_after: QUIET,
    }
}

/// Run `req` against its server, bounded by [`HUNG`].
async fn run(req: &NativeDownload<'_>) -> Result<(), NativeError> {
    tokio::time::timeout(HUNG, download_file(&build_client(), req))
        .await
        .expect("a quiet connection must not hold the download for ever")
}

/// The connection sends part of the body and then nothing, without closing.
/// The download drops it, asks again for the rest, and the file is whole.
#[tokio::test]
async fn a_quiet_connection_is_dropped_and_the_download_carries_on() {
    let body = body_of(8192);
    let sent = 3000;
    let server = TestServer::start(vec![
        Reply::Stalls {
            body: body.clone(),
            limit: sent,
        },
        Reply::File {
            body: body.clone(),
            etag: Some(sha256_of(&body)),
        },
    ])
    .await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("the download carries on over a new connection and completes");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body, "the file is whole");
    let ranges = server.ranges();
    assert_eq!(ranges[0], None, "the first connection asks for the file");
    assert_eq!(
        ranges[1],
        Some(format!("bytes={sent}-")),
        "the second asks only for what is missing"
    );
    let last = *seen.readings.lock().unwrap().last().expect("readings");
    assert_eq!(
        last.received,
        body.len() as u64,
        "what arrived over both connections is counted, so the speed goes on"
    );
    assert!(
        seen.notices()
            .iter()
            .any(|n| n == "no data for 1s, reconnecting (1 of 3)…"),
        "the wait is said while it lasts: {:?}",
        seen.notices()
    );
}

/// Every connection after the first sends nothing at all. The download
/// gives up after its reconnects, says why in one sentence, and keeps the
/// part for the next try.
#[tokio::test]
async fn a_server_that_stays_quiet_ends_in_the_stall_sentence() {
    let body = body_of(8192);
    let sent = 1000;
    let server = TestServer::start(vec![
        Reply::Stalls {
            body: body.clone(),
            limit: sent,
        },
        Reply::NeverAnswers,
    ])
    .await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());
    let url = server.url();

    let err = run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect_err("a server that never sends the rest cannot complete the file");

    assert_eq!(
        server.request_count(),
        1 + RECONNECTS,
        "the first connection and {RECONNECTS} more"
    );
    assert_eq!(
        to_download_error(err).to_string(),
        "no data arrived for 4s, so the download stopped; trying again carries on \
         from the 1000 B already downloaded"
    );
    assert_eq!(
        std::fs::metadata(fx.part()).map(|m| m.len()).ok(),
        Some(sent as u64),
        "the part is kept, so trying again carries on from it"
    );
}

/// A server that ignores `Range` and goes quiet at the same byte every
/// time sends bytes on every connection, but the part never gets past
/// where it was: that is no progress, and the reconnects run out.
#[tokio::test]
async fn bytes_that_only_make_up_a_restart_are_no_progress() {
    let body = body_of(8192);
    let server = TestServer::start(vec![Reply::Stalls {
        body: body.clone(),
        limit: 1000,
    }])
    .await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect_err("the part never gets further, so the download gives up");

    assert_eq!(server.request_count(), 1 + RECONNECTS);
}

/// Each connection brings some bytes before it goes quiet. Progress starts
/// the count again, so more stalls than the bound do not end the download.
#[tokio::test]
async fn stalls_that_each_bring_bytes_do_not_use_up_the_reconnects() {
    let body = body_of(8192);
    let chunk = 1000;
    let mut script = vec![
        Reply::StallsFrom {
            body: body.clone(),
            chunk,
        };
        RECONNECTS + 2
    ];
    script.push(Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    });
    let server = TestServer::start(script).await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("stalls with progress between them do not end the download");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert!(server.request_count() >= RECONNECTS + 3);
}

/// A cancel while the connection is quiet ends the transfer at once, with
/// what arrived in the part.
#[tokio::test]
async fn a_cancel_on_a_quiet_connection_ends_the_transfer_at_once() {
    let body = body_of(8192);
    let sent = 3000;
    let server = TestServer::start(vec![Reply::Stalls {
        body: body.clone(),
        limit: sent,
    }])
    .await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());
    let url = server.url();
    let cancel = CancellationToken::new();
    let request = NativeDownload {
        cancel: Some(cancel.clone()),
        stall_after: STALL_AFTER,
        ..quiet_request(&url, &fx, body.len(), &seen)
    };

    let client = build_client();
    let transfer = download_file(&client, &request);
    let cancelled = async {
        while seen
            .readings
            .lock()
            .unwrap()
            .last()
            .map_or(0, |r| r.received)
            < sent as u64
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let at = Instant::now();
        cancel.cancel();
        at
    };
    let (result, cancelled_at) =
        tokio::time::timeout(HUNG, async { tokio::join!(transfer, cancelled) })
            .await
            .expect("the cancelled transfer ends");

    assert!(
        matches!(result, Err(NativeError::Cancelled)),
        "got {result:?}"
    );
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(2),
        "the cancel is answered at once, took {:?}",
        cancelled_at.elapsed()
    );
    assert_eq!(std::fs::metadata(fx.part()).unwrap().len(), sent as u64);
}

// ============================================================================
// A network that goes away
// ============================================================================

/// A server whose first connection gets the first `sent` bytes of `body`
/// and then nothing, held open. It stops listening once it has taken that
/// connection, so every reconnect is refused, as when the network is down;
/// with `back_after`, it listens on the same port again that long after,
/// honouring `Range`. Answers its address.
async fn goes_away(body: Vec<u8>, sent: usize, back_after: Option<Duration>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let etag = sha256_of(&body);
    tokio::spawn(async move {
        let incoming = gglib_test_http::incoming(&listener).await.unwrap();
        drop(listener);
        let (first_body, first_etag) = (body.clone(), etag.clone());
        tokio::spawn(async move {
            let (_request, mut socket) = incoming.request().await.unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Linked-Etag: \"{first_etag}\"\r\nConnection: close\r\n\r\n",
                first_body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(&first_body[..sent]).await.unwrap();
            socket.flush().await.unwrap();
            std::future::pending::<()>().await;
        });

        let Some(after) = back_after else { return };
        tokio::time::sleep(after).await;
        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.set_reuseaddr(true).unwrap();
        socket.bind(addr).unwrap();
        let listener = socket.listen(16).unwrap();
        while let Ok((request, mut socket)) = gglib_test_http::accept(&listener).await {
            let start: usize = request
                .header("range")
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.split('-').next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let head = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nX-Linked-Etag: \"{etag}\"\r\nConnection: close\r\n\r\n",
                body.len() - start,
                body.len() - 1,
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&body[start..]).await;
            let _ = socket.flush().await;
        }
    });
    format!("http://{addr}/model.gguf")
}

/// The connection goes quiet, and then the network is down: every
/// reconnect is refused, until it comes back. The refused reconnects are
/// tried again, spaced a quiet time apart, and the download carries on
/// from its part once the server can be reached.
#[tokio::test]
async fn a_download_reconnects_once_the_network_comes_back() {
    let body = body_of(8192);
    let url = goes_away(body.clone(), 3000, Some(Duration::from_millis(1500))).await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("the download carries on once the server is back");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body, "the file is whole");
    let notices = seen.notices();
    assert!(
        notices
            .iter()
            .any(|n| n == "no data for 1s, reconnecting (2 of 3)…"),
        "a refused reconnect is counted and said: {notices:?}"
    );
}

/// The network never comes back: the refused reconnects run out, and the
/// download ends in the stall's sentence, not the transport's error.
#[tokio::test]
async fn a_network_that_never_comes_back_ends_in_the_stall_sentence() {
    let body = body_of(8192);
    let url = goes_away(body.clone(), 3000, None).await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());

    let started = Instant::now();
    let err = run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect_err("a server that cannot be reached cannot complete the file");

    assert_eq!(
        to_download_error(err).to_string(),
        "no data arrived for 3s, so the download stopped; trying again carries on \
         from the 2.93 KiB already downloaded"
    );
    assert!(
        started.elapsed() < QUIET * 6,
        "it gives up within its bound, took {:?}",
        started.elapsed()
    );
    assert_eq!(
        seen.notices(),
        [
            "no data for 1s, reconnecting (1 of 3)…",
            "no data for 1s, reconnecting (2 of 3)…",
            "no data for 1s, reconnecting (3 of 3)…",
        ]
    );
    assert_eq!(std::fs::metadata(fx.part()).unwrap().len(), 3000);
}

/// The connection breaks mid-body (an error, not silence): the download
/// reconnects at once, from what arrived, and says so while it does.
#[tokio::test]
async fn a_connection_that_breaks_mid_body_is_reconnected_at_once() {
    let body = body_of(8192);
    let server = TestServer::start(vec![
        Reply::Truncated {
            body: body.clone(),
            limit: 3000,
        },
        Reply::File {
            body: body.clone(),
            etag: Some(sha256_of(&body)),
        },
    ])
    .await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());
    let url = server.url();

    let started = Instant::now();
    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("the download carries on over a new connection");

    assert!(started.elapsed() < QUIET, "no wait: it brought bytes");
    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert_eq!(server.ranges()[1], Some("bytes=3000-".to_string()));
    assert_eq!(
        seen.notices().first().map(String::as_str),
        Some("connection lost, reconnecting (1 of 3)…")
    );
}

/// The first connection of a download with nothing on disk that cannot be
/// made fails at once: there is nothing to carry on from, and the host may
/// simply be wrong.
#[tokio::test]
async fn a_first_connection_that_cannot_be_made_fails_at_once() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://{addr}/model.gguf");
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());

    let started = Instant::now();
    let err = run(&quiet_request(&url, &fx, 8192, &seen))
        .await
        .expect_err("nothing answers");

    assert!(started.elapsed() < QUIET, "took {:?}", started.elapsed());
    assert!(
        err.to_string()
            .starts_with("Network error: could not connect to 127.0.0.1: "),
        "the cause is said plainly: {err}"
    );
    assert!(seen.notices().is_empty());
}

// ============================================================================
// A 206 that cannot be placed
// ============================================================================

/// Resume from a part of 1000 bytes against `reply`, and answer the error
/// and the part's length after.
async fn resume_against(reply: Reply, expected_size: usize) -> (NativeError, u64) {
    let body = body_of(8192);
    let fx = Fixture::new();
    std::fs::write(fx.part(), &body[..1000]).unwrap();
    let server = TestServer::start(vec![reply]).await;
    let seen = Arc::new(Seen::default());
    let url = server.url();

    let err = run(&quiet_request(&url, &fx, expected_size, &seen))
        .await
        .expect_err("an answer that cannot be placed in the file is refused");
    (err, std::fs::metadata(fx.part()).unwrap().len())
}

/// A 206 that starts at another byte than the one asked for is refused,
/// and nothing is appended to the part.
#[tokio::test]
async fn a_partial_answer_from_the_wrong_byte_is_refused() {
    let body = body_of(8192);
    let (err, kept) = resume_against(Reply::PartialFromStart { body }, 8192).await;

    assert!(
        err.to_string()
            .contains("asked for the file from byte 1000, and the server sent it from byte 0"),
        "got {err}"
    );
    assert_eq!(kept, 1000);
}

/// A 206 that does not say where it starts cannot be placed either.
#[tokio::test]
async fn a_partial_answer_that_does_not_say_where_it_starts_is_refused() {
    let body = body_of(8192);
    let (err, kept) = resume_against(Reply::PartialWithoutRange { body }, 8192).await;

    assert!(
        err.to_string()
            .contains("the server did not say where its answer starts"),
        "got {err}"
    );
    assert_eq!(kept, 1000);
}

/// A 206 for a file of another size than the one expected is of another
/// file.
#[tokio::test]
async fn a_partial_answer_for_a_file_of_another_size_is_refused() {
    let body = body_of(8192);
    let (err, kept) = resume_against(Reply::File { body, etag: None }, 9000).await;

    assert!(
        err.to_string()
            .contains("the server's file is 8192 bytes, not the 9000 expected"),
        "got {err}"
    );
    assert_eq!(kept, 1000);
}

// ============================================================================
// A file that changes between connections
// ============================================================================

/// The first connection goes quiet part-way; the next one's redirect
/// carries another digest: the file changed on the server. The part holds
/// the start of the old file, so it is not carried on from: the download
/// starts over, says so, and ends with the new file whole.
#[tokio::test]
async fn a_file_that_changed_between_connections_is_started_over_not_spliced() {
    let old = body_of(8192);
    let new: Vec<u8> = old.iter().map(|b| b ^ 0x5a).collect();
    let server = TestServer::start(vec![
        Reply::RedirectWithLinkedEtag {
            linked_etag: sha256_of(&old),
        },
        Reply::StallsFrom {
            body: old.clone(),
            chunk: 3000,
        },
        Reply::RedirectWithLinkedEtag {
            linked_etag: sha256_of(&new),
        },
        Reply::CdnFile {
            body: new.clone(),
            decoy_etag: "0".repeat(64),
        },
        Reply::RedirectWithLinkedEtag {
            linked_etag: sha256_of(&new),
        },
        Reply::CdnFile {
            body: new.clone(),
            decoy_etag: "0".repeat(64),
        },
    ])
    .await;
    let fx = Fixture::new();
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, new.len(), &seen))
        .await
        .expect("the new file is fetched whole");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), new);
    assert_eq!(
        server.ranges().last().unwrap(),
        &None,
        "the last connection asks for the whole file"
    );
    assert!(
        seen.notices()
            .iter()
            .any(|n| n == "the file changed on the server; starting it over…"),
        "{:?}",
        seen.notices()
    );
}

// ============================================================================
// A resume checks the part off the async thread, and before it connects
// ============================================================================

type Probe = Arc<dyn Fn() + Send + Sync>;

fn probes() -> &'static Mutex<HashMap<PathBuf, Probe>> {
    static PROBES: OnceLock<Mutex<HashMap<PathBuf, Probe>>> = OnceLock::new();
    PROBES.get_or_init(Mutex::default)
}

/// Called before each block of the part at `path` is read through the
/// hasher: runs the probe a test set for `path`, if any.
pub(super) fn run_seed_probe(path: &Path) {
    let probe = probes().lock().unwrap().get(path).cloned();
    if let Some(probe) = probe {
        probe();
    }
}

fn set_probe(path: &Path, probe: impl Fn() + Send + Sync + 'static) {
    probes()
        .lock()
        .unwrap()
        .insert(path.to_path_buf(), Arc::new(probe));
}

/// While the part at `path` is read through the hasher, wait for a task on
/// this test's runtime to answer. The runtime is current-thread, so the
/// task can answer only if the reading runs off it. Answers whether the
/// wait timed out: the reading held the runtime, as it held `/health`.
fn runtime_must_turn_while_the_part_is_read(path: &Path) -> Arc<AtomicBool> {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let rx = Mutex::new(rx);
    let reading = Arc::new(AtomicBool::new(false));
    let held = Arc::new(AtomicBool::new(false));

    let (reading_in_probe, held_in_probe) = (Arc::clone(&reading), Arc::clone(&held));
    set_probe(path, move || {
        // The first block is the one waited on.
        if reading_in_probe.swap(true, Ordering::SeqCst) {
            return;
        }
        if rx
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .is_err()
        {
            held_in_probe.store(true, Ordering::SeqCst);
        }
    });

    tokio::spawn(async move {
        while !reading.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        let _ = tx.send(());
    });
    held
}

/// A resume reads the part already on disk through the hasher. That read
/// is gigabytes on a large model, and it must not hold the runtime that
/// answers `/health`.
#[tokio::test]
async fn a_resume_reads_the_part_off_the_async_thread() {
    let body = body_of(64 * 1024);
    let fx = Fixture::new();
    std::fs::write(fx.part(), &body[..40_000]).unwrap();
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    }])
    .await;
    let held = runtime_must_turn_while_the_part_is_read(&fx.part());
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("the resume completes");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert!(
        !held.load(Ordering::SeqCst),
        "the runtime stood still while the part was read"
    );
    assert_eq!(
        seen.notices().first().map(String::as_str),
        Some("checking the 39.06 KiB already downloaded…")
    );
}

/// A part that is already whole (the server answers 416) is read through
/// the hasher too, and off the async thread the same way.
#[tokio::test]
async fn a_whole_part_is_read_off_the_async_thread() {
    let body = body_of(64 * 1024);
    let fx = Fixture::new();
    std::fs::write(fx.part(), &body).unwrap();
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    }])
    .await;
    let held = runtime_must_turn_while_the_part_is_read(&fx.part());
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("a whole part is published");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert!(
        !held.load(Ordering::SeqCst),
        "the runtime stood still while the part was read"
    );
}

/// The part is read through the hasher before the connection opens, so no
/// connection sits unread while it is: a server that resets a connection
/// whose answer is not read in time is still resumed from, however long
/// the part takes to read.
#[tokio::test]
async fn a_resume_reads_the_part_before_it_connects() {
    let body = body_of(24 << 20);
    let fx = Fixture::new();
    std::fs::write(fx.part(), &body[..1000]).unwrap();
    let server = TestServer::start(vec![Reply::DropsIfUnread {
        body: body.clone(),
        within: Duration::from_secs(1),
    }])
    .await;
    // A part that takes three seconds to read, as a large one on a slow disk.
    let first = AtomicBool::new(true);
    set_probe(&fx.part(), move || {
        if first.swap(false, Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(3));
        }
    });
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("the server's answer is read as it comes, and the resume completes");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert_eq!(server.ranges(), [Some("bytes=1000-".to_string())]);
}

/// The Hub's shape: every connection starts at `resolve/`, whose redirect
/// carries the file's SHA-256, and the bytes come from the CDN. The first
/// connection goes quiet part-way; the second carries the first's hash on
/// rather than reading the part again, and the whole file still verifies.
#[tokio::test]
async fn a_reconnect_on_the_hub_carries_the_hash_on_and_the_file_verifies() {
    let body = body_of(8192);
    let digest = sha256_of(&body);
    let server = TestServer::start(vec![
        Reply::RedirectWithLinkedEtag {
            linked_etag: digest.clone(),
        },
        Reply::StallsFrom {
            body: body.clone(),
            chunk: 3000,
        },
        Reply::RedirectWithLinkedEtag {
            linked_etag: digest,
        },
        Reply::CdnFile {
            body: body.clone(),
            decoy_etag: "0".repeat(64),
        },
    ])
    .await;
    let fx = Fixture::new();
    let read_again = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&read_again);
    set_probe(&fx.part(), move || flag.store(true, Ordering::SeqCst));
    let seen = Arc::new(Seen::default());
    let url = server.url();

    run(&quiet_request(&url, &fx, body.len(), &seen))
        .await
        .expect("the file verifies against the digest the redirect carried");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert_eq!(
        server.ranges()[2..],
        [
            Some("bytes=3000-".to_string()),
            Some("bytes=3000-".to_string())
        ],
        "the reconnect asks for the rest, at resolve/ and at the CDN"
    );
    assert!(
        !read_again.load(Ordering::SeqCst),
        "the reconnect read the part through the hasher again"
    );
}

/// A cancel while a part is read through the hasher stops the read between
/// two blocks, rather than leaving gigabytes of it to finish on a blocking
/// thread.
#[test]
fn a_cancel_stops_the_read_of_a_part_between_two_blocks() {
    let fx = Fixture::new();
    std::fs::write(fx.part(), body_of(4 * HASH_BLOCK_BYTES)).unwrap();
    let cancel = CancellationToken::new();
    let blocks = Arc::new(AtomicUsize::new(0));
    let (counted, token) = (Arc::clone(&blocks), cancel.clone());
    set_probe(&fx.part(), move || {
        // The cancel comes once the first block has been read.
        if counted.fetch_add(1, Ordering::SeqCst) == 1 {
            token.cancel();
        }
    });

    let result = seed_hasher(
        &fx.part(),
        4 * HASH_BLOCK_BYTES as u64,
        &mut Sha256::new(),
        Some(&cancel),
    );

    assert!(
        matches!(result, Err(NativeError::Cancelled)),
        "got {result:?}"
    );
    assert_eq!(
        blocks.load(Ordering::SeqCst),
        2,
        "one block read, and the read stopped before the next"
    );
}
