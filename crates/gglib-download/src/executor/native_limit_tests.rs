//! A transfer at Hugging Face's rate limit, against the loopback server's
//! replay of the refusal the Hub answered on 2026-11-23.
//!
//! The waits are real seconds: the server is a real socket, and a paused
//! clock would run each connection's quiet time out from under it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gglib_core::hub_limit::{HubBucket, HubGate, HubLimit, waiting_sentence};
use tokio_util::sync::CancellationToken;

use super::super::native::tests::{Fixture, Reply, TestServer, body_of, request, sha256_of};
use super::super::native::{NativeError, build_client, download_gated};

/// Every note shown on the row, in order, each with the count of requests
/// the server had had when it was shown.
type Notes = Arc<Mutex<Vec<(String, usize)>>>;

/// A sink for the row's notes that records each beside `requests`.
fn noting(requests: Arc<AtomicUsize>) -> (crate::cli_exec::NoticeCallback, Notes) {
    let notes = Notes::default();
    let sink = Arc::clone(&notes);
    let notice = Arc::new(move |note: &str| {
        let asked = requests.load(Ordering::SeqCst);
        sink.lock().unwrap().push((note.to_owned(), asked));
    });
    (notice, notes)
}

/// The file, whole, with its digest.
fn the_file(body: &[u8]) -> Reply {
    Reply::File {
        body: body.to_vec(),
        etag: Some(sha256_of(body)),
    }
}

/// A refusal is waited out with the time left on the row, and the file is
/// asked for again: the download completes, having spent no reconnect.
#[tokio::test]
async fn a_refused_download_waits_and_completes() {
    let body = body_of(4096);
    let server = TestServer::start(vec![Reply::Limited { t: 1 }, the_file(&body)]).await;
    let fx = Fixture::new();
    let (notice, notes) = noting(server.counter());
    let url = server.resolve_url();
    let req = super::super::native::NativeDownload {
        notice: Some(notice),
        ..request(&url, &fx.dest, Some(body.len() as u64), None)
    };

    download_gated(&build_client(), &req, &HubGate::new())
        .await
        .expect("the download completes once the limit lifts");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert_eq!(server.request_count(), 2, "asked once, and once again");
    let notes = notes.lock().unwrap().clone();
    assert_eq!(
        notes.first(),
        Some(&(waiting_sentence(Duration::from_secs(1)), 1)),
        "the wait is shown once refused, before the file is asked for again"
    );
    assert!(
        notes.iter().all(|(note, _)| !note.contains("reconnecting")),
        "a refusal spends no reconnect: {notes:?}"
    );
}

/// A cancel while the limit is waited out ends the download at once, as
/// cancelled, with nothing more asked.
#[tokio::test]
async fn a_cancel_ends_the_wait() {
    let server = TestServer::start(vec![Reply::Limited { t: 600 }]).await;
    let fx = Fixture::new();
    let cancel = CancellationToken::new();
    let on_wait = cancel.clone();
    let notice: crate::cli_exec::NoticeCallback = Arc::new(move |_: &str| on_wait.cancel());
    let url = server.resolve_url();
    let req = super::super::native::NativeDownload {
        notice: Some(notice),
        cancel: Some(cancel),
        ..request(&url, &fx.dest, None, None)
    };

    // A ten-minute wait, so ending well inside a minute is the cancel's.
    let ended = tokio::time::timeout(
        Duration::from_mins(1),
        download_gated(&build_client(), &req, &HubGate::new()),
    )
    .await
    .expect("the cancel ends the wait");

    assert!(
        matches!(ended, Err(NativeError::Cancelled)),
        "got {ended:?}"
    );
    assert_eq!(server.request_count(), 1);
    assert!(!fx.dest.exists());
}

/// Refused again and again with nothing arriving, the download gives up
/// on the fifth refusal, saying the Hub kept limiting it.
#[tokio::test]
async fn a_download_refused_five_times_in_a_row_gives_up() {
    let server = TestServer::start(vec![Reply::Limited { t: 1 }]).await;
    let fx = Fixture::new();
    let url = server.resolve_url();
    let req = request(&url, &fx.dest, None, None);

    let ended = download_gated(&build_client(), &req, &HubGate::new()).await;

    let Err(e @ NativeError::KeptLimited) = ended else {
        panic!("expected the give-up, got {ended:?}");
    };
    assert_eq!(
        e.to_string(),
        "Hugging Face kept rate limiting the download: it refused it 5 times in a row"
    );
    assert_eq!(server.request_count(), 5);
}

/// With the file allowance already closed, as another request left it,
/// nothing is asked until it opens: the wait is shown first, with no
/// request made, and then the file is fetched with one.
#[tokio::test]
async fn a_closed_allowance_is_not_asked_until_it_opens() {
    let body = body_of(2048);
    let server = TestServer::start(vec![the_file(&body)]).await;
    let fx = Fixture::new();
    let gate = HubGate::new();
    gate.note(
        HubLimit {
            bucket: HubBucket::Resolvers,
            remaining: 0,
            resets_in: Duration::from_secs(1),
        },
        tokio::time::Instant::now(),
    );
    let (notice, notes) = noting(server.counter());
    let url = server.resolve_url();
    let req = super::super::native::NativeDownload {
        notice: Some(notice),
        ..request(&url, &fx.dest, Some(body.len() as u64), None)
    };

    download_gated(&build_client(), &req, &gate)
        .await
        .expect("the download completes once the allowance opens");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert_eq!(server.request_count(), 1);
    let notes = notes.lock().unwrap().clone();
    assert_eq!(
        notes.first(),
        Some(&(waiting_sentence(Duration::from_secs(1)), 0)),
        "the wait comes before any request"
    );
}

/// A refusal closes the allowance in the gate it shares, so the next file
/// of the download, or of any other, waits rather than asks.
#[tokio::test]
async fn a_refusal_closes_the_shared_gate() {
    let server = TestServer::start(vec![Reply::Limited { t: 120 }]).await;
    let fx = Fixture::new();
    let gate = HubGate::new();
    let cancel = CancellationToken::new();
    let on_wait = cancel.clone();
    let notice: crate::cli_exec::NoticeCallback = Arc::new(move |_: &str| on_wait.cancel());
    let url = server.resolve_url();
    let req = super::super::native::NativeDownload {
        notice: Some(notice),
        cancel: Some(cancel),
        ..request(&url, &fx.dest, None, None)
    };

    let ended = download_gated(&build_client(), &req, &gate).await;

    assert!(
        matches!(ended, Err(NativeError::Cancelled)),
        "got {ended:?}"
    );
    let left = gate
        .closed(HubBucket::Resolvers, tokio::time::Instant::now())
        .expect("the file allowance is closed");
    assert!(
        left > Duration::from_mins(1),
        "closed until its reset: {left:?}"
    );
    assert_eq!(
        gate.closed(HubBucket::Api, tokio::time::Instant::now()),
        None
    );
}
