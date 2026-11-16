//! A connection that goes quiet, and a resume that checks a large part.
//!
//! The quiet time is injected short (`QUIET`): production sets it from
//! [`STALL_AFTER`], and the suite does not wait tens of seconds for it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::tests::{Fixture, Reply, TestServer, body_of, sha256_of};
use super::*;
use crate::executor::to_download_error;

/// The quiet time the tests run with.
const QUIET: Duration = Duration::from_millis(250);

/// How long a test may take before it counts as hung. A download that waits
/// on a quiet connection for ever fails here rather than hanging the suite.
const HUNG: Duration = Duration::from_secs(10);

/// Reconnects after a quiet connection before the download gives up.
const RECONNECTS: usize = 3;

/// What one download reported: every reading and every note.
#[derive(Default)]
struct Seen {
    readings: Mutex<Vec<RawProgress>>,
    notices: Mutex<Vec<String>>,
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

    tokio::time::timeout(
        HUNG,
        download_file(
            &build_client(),
            &quiet_request(&url, &fx, body.len(), &seen),
        ),
    )
    .await
    .expect("a quiet connection must not hold the download for ever")
    .expect("the download carries on over a new connection and completes");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body, "the file is whole");
    assert_eq!(
        server.ranges(),
        [None, Some(format!("bytes={sent}-"))],
        "the second connection asks only for what is missing"
    );
    let last = *seen.readings.lock().unwrap().last().expect("readings");
    assert_eq!(
        last.received,
        body.len() as u64,
        "what arrived over both connections is counted, so the speed goes on"
    );
    let notices = seen.notices.lock().unwrap().clone();
    assert!(
        notices.iter().any(|n| n.contains("reconnecting (1 of 3)")),
        "the wait is said while it lasts: {notices:?}"
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

    let started = Instant::now();
    let err = tokio::time::timeout(
        HUNG,
        download_file(
            &build_client(),
            &quiet_request(&url, &fx, body.len(), &seen),
        ),
    )
    .await
    .expect("a server that stays quiet must not hold the download for ever")
    .expect_err("a server that never sends the rest cannot complete the file");

    assert!(
        started.elapsed() < QUIET * u32::try_from(RECONNECTS + 1).unwrap() * 4,
        "it gives up within its bound, took {:?}",
        started.elapsed()
    );
    assert_eq!(
        server.request_count(),
        1 + RECONNECTS,
        "the first connection and {RECONNECTS} more"
    );
    assert_eq!(
        to_download_error(err).to_string(),
        "no data arrived for 1s, so the download stopped; trying again carries on \
         from the 1000 B already downloaded"
    );
    assert_eq!(
        std::fs::metadata(fx.part()).map(|m| m.len()).ok(),
        Some(sent as u64),
        "the part is kept, so trying again carries on from it"
    );
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

    tokio::time::timeout(
        HUNG,
        download_file(
            &build_client(),
            &quiet_request(&url, &fx, body.len(), &seen),
        ),
    )
    .await
    .expect("a quiet connection must not hold the download for ever")
    .expect("stalls with progress between them do not end the download");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert_eq!(server.request_count(), RECONNECTS + 3);
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
// A resume checks the part off the async thread
// ============================================================================

type Probe = Arc<dyn Fn() + Send + Sync>;

fn probes() -> &'static Mutex<HashMap<PathBuf, Probe>> {
    static PROBES: OnceLock<Mutex<HashMap<PathBuf, Probe>>> = OnceLock::new();
    PROBES.get_or_init(Mutex::default)
}

/// Called where the bytes already on disk are read through the hasher: runs
/// the probe a test set for `path`, if any.
pub(super) fn run_seed_probe(path: &Path) {
    let probe = probes().lock().unwrap().get(path).cloned();
    if let Some(probe) = probe {
        probe();
    }
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
    let probe: Probe = Arc::new(move || {
        reading_in_probe.store(true, Ordering::SeqCst);
        if rx
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .is_err()
        {
            held_in_probe.store(true, Ordering::SeqCst);
        }
    });
    probes().lock().unwrap().insert(path.to_path_buf(), probe);

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

    tokio::time::timeout(
        HUNG,
        download_file(
            &build_client(),
            &quiet_request(&url, &fx, body.len(), &seen),
        ),
    )
    .await
    .expect("the resume ends")
    .expect("the resume completes");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert!(
        !held.load(Ordering::SeqCst),
        "the runtime stood still while the part was read"
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

    tokio::time::timeout(
        HUNG,
        download_file(
            &build_client(),
            &quiet_request(&url, &fx, body.len(), &seen),
        ),
    )
    .await
    .expect("the check ends")
    .expect("a whole part is published");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert!(
        !held.load(Ordering::SeqCst),
        "the runtime stood still while the part was read"
    );
}
