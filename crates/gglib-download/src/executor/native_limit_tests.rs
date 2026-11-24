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

/// [`noting`], whose first note also cancels the download with `cancel`.
fn noting_then_cancelling(
    requests: Arc<AtomicUsize>,
    cancel: CancellationToken,
) -> (crate::cli_exec::NoticeCallback, Notes) {
    let (record, notes) = noting(requests);
    let notice = Arc::new(move |note: &str| {
        record(note);
        cancel.cancel();
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
/// nothing is asked while it stays closed: the wait is shown with no
/// request made. (A download that waits and then fetches is
/// [`a_refused_download_waits_and_completes`].)
#[tokio::test]
async fn a_closed_allowance_is_not_asked_while_it_is_closed() {
    let server = TestServer::start(vec![the_file(&body_of(2048))]).await;
    let fx = Fixture::new();
    let gate = HubGate::new();
    gate.note(
        HubLimit {
            bucket: HubBucket::Resolvers,
            remaining: 0,
            resets_in: Duration::from_mins(2),
        },
        tokio::time::Instant::now(),
    );
    let cancel = CancellationToken::new();
    let (notice, notes) = noting_then_cancelling(server.counter(), cancel.clone());
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
    assert_eq!(server.request_count(), 0);
    let notes = notes.lock().unwrap().clone();
    let (first, asked) = notes.first().expect("the wait is shown");
    assert!(
        first.starts_with("waiting for Hugging Face's rate limit, "),
        "{first}"
    );
    assert_eq!(*asked, 0, "the wait comes before any request");
}

/// A refusal on a redirect's hop that names no allowance is filed under the
/// one the download asked for, a file's, not under what the hop's own path
/// would read as.
#[tokio::test]
async fn a_refusal_on_a_redirect_is_filed_under_the_allowance_asked() {
    let server = TestServer::start(vec![
        Reply::RedirectWithLinkedEtag {
            linked_etag: sha256_of(b"any"),
        },
        Reply::Status(429),
    ])
    .await;
    let fx = Fixture::new();
    let gate = HubGate::new();
    let cancel = CancellationToken::new();
    let (notice, _) = noting_then_cancelling(server.counter(), cancel.clone());
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
    assert_eq!(server.request_count(), 2, "the hop and its refusal");
    let now = tokio::time::Instant::now();
    assert!(gate.closed(HubBucket::Resolvers, now).is_some());
    assert_eq!(gate.closed(HubBucket::Pages, now), None);
}

/// A refusal after bytes have arrived carries on from them once it is
/// waited out, and bytes start the count of refusals in a row again: four
/// refusals, a connection that brings part of the file and breaks, four
/// more, and the file, which a count kept across the bytes would have
/// given up on.
#[tokio::test]
async fn a_refusal_after_bytes_carries_on_from_them_and_counts_again() {
    let body = body_of(8192);
    let kept = 3000;
    let mut script = vec![Reply::Limited { t: 1 }; 4];
    script.push(Reply::Truncated {
        body: body.clone(),
        limit: kept,
    });
    script.extend(vec![Reply::Limited { t: 1 }; 4]);
    script.push(the_file(&body));
    let server = TestServer::start(script).await;
    let fx = Fixture::new();
    let url = server.resolve_url();
    let req = request(&url, &fx.dest, Some(body.len() as u64), None);

    download_gated(&build_client(), &req, &HubGate::new())
        .await
        .expect("the download completes");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    let resumed = Some(format!("bytes={kept}-"));
    let mut asked = vec![None; 5];
    asked.extend(vec![resumed; 5]);
    assert_eq!(
        server.ranges(),
        asked,
        "each ask after the bytes is for the rest"
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
