//! Tests for the native downloader, against a scripted loopback server.
//!
//! Hand-rolled on a raw [`TcpListener`] rather than reaching for `wiremock` or
//! `hyper`: `check_boundaries.sh` forbids `hyper` anywhere in `gglib-download`'s
//! dependency tree and `cargo tree --depth 1` sees dev-dependencies too, so a
//! ready-made harness would fail CI. Everything here is built from `tokio`,
//! which the crate already has.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::*;

// ============================================================================
// Test server
// ============================================================================

/// How the server should answer one request.
#[derive(Clone)]
enum Reply {
    /// Serve `body`, honouring `Range` and advertising `etag` when set.
    File { body: Vec<u8>, etag: Option<String> },
    /// Serve only the first `limit` bytes of `body`, then drop the connection
    /// mid-transfer without sending the rest. Simulates an interrupted download.
    Truncated { body: Vec<u8>, limit: usize },
    /// Answer with a bare status line.
    Status(u16),
    /// Ignore `Range` and always send the whole body with `200`.
    IgnoresRange { body: Vec<u8> },
}

struct TestServer {
    base_url: String,
    /// `Range` header value seen on each request, in order.
    ranges: Arc<Mutex<Vec<Option<String>>>>,
    requests: Arc<AtomicUsize>,
    handle: JoinHandle<()>,
}

impl TestServer {
    /// Bind an ephemeral loopback port. Replies are served in order; the last
    /// one repeats for any further requests.
    async fn start(script: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral loopback port");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("resolve bound address")
        );

        let ranges = Arc::new(Mutex::new(Vec::new()));
        let requests = Arc::new(AtomicUsize::new(0));
        let handle = tokio::spawn(serve(
            listener,
            script,
            Arc::clone(&ranges),
            Arc::clone(&requests),
        ));

        Self {
            base_url,
            ranges,
            requests,
            handle,
        }
    }

    fn url(&self) -> String {
        format!("{}/model.gguf", self.base_url)
    }

    fn ranges(&self) -> Vec<Option<String>> {
        self.ranges.lock().unwrap().clone()
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn serve(
    listener: TcpListener,
    script: Vec<Reply>,
    ranges: Arc<Mutex<Vec<Option<String>>>>,
    requests: Arc<AtomicUsize>,
) {
    loop {
        let Ok((socket, _)) = listener.accept().await else {
            return;
        };
        let n = requests.fetch_add(1, Ordering::SeqCst);
        let reply = script
            .get(n)
            .or_else(|| script.last())
            .cloned()
            .unwrap_or(Reply::Status(500));

        let ranges = Arc::clone(&ranges);
        tokio::spawn(async move {
            let _ = handle_one(socket, reply, ranges).await;
        });
    }
}

async fn handle_one(
    socket: tokio::net::TcpStream,
    reply: Reply,
    ranges: Arc<Mutex<Vec<Option<String>>>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(socket);

    // Read the request head, capturing any Range header.
    let mut range: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
        }
    }
    ranges.lock().unwrap().push(range.clone());

    let start: usize = range
        .as_deref()
        .and_then(|r| r.strip_prefix("bytes="))
        .and_then(|r| r.split('-').next())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let socket = reader.get_mut();

    match reply {
        Reply::Status(code) => {
            let head =
                format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            socket.write_all(head.as_bytes()).await?;
        }
        Reply::IgnoresRange { body } => {
            write_full(socket, &body, None).await?;
        }
        Reply::File { body, etag } => {
            if start >= body.len() {
                let head = format!(
                    "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n{}Connection: close\r\n\r\n",
                    etag_header(etag.as_deref())
                );
                socket.write_all(head.as_bytes()).await?;
            } else if start > 0 {
                let slice = &body[start..];
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n{}Connection: close\r\n\r\n",
                    slice.len(),
                    start,
                    body.len() - 1,
                    body.len(),
                    etag_header(etag.as_deref())
                );
                socket.write_all(head.as_bytes()).await?;
                socket.write_all(slice).await?;
            } else {
                write_full(socket, &body, etag.as_deref()).await?;
            }
        }
        Reply::Truncated { body, limit } => {
            // Advertise the full length, then send less and hang up. The client
            // must treat what it got as a resumable partial.
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await?;
            socket.write_all(&body[..limit]).await?;
        }
    }

    socket.flush().await?;
    Ok(())
}

async fn write_full(
    socket: &mut tokio::net::TcpStream,
    body: &[u8],
    etag: Option<&str>,
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
        body.len(),
        etag_header(etag)
    );
    socket.write_all(head.as_bytes()).await?;
    socket.write_all(body).await
}

fn etag_header(etag: Option<&str>) -> String {
    etag.map_or_else(String::new, |e| format!("X-Linked-Etag: \"{e}\"\r\n"))
}

// ============================================================================
// Fixtures
// ============================================================================

fn body_of(len: usize) -> Vec<u8> {
    // A repeating non-power-of-two pattern, so a mis-sliced resume shows up as a
    // content mismatch rather than lining up by accident.
    (0..len)
        .map(|i| u8::try_from(i % 251).unwrap_or_default())
        .collect()
}

/// Every `(downloaded, total)` pair the downloader reported, in order.
type ProgressLog = Arc<Mutex<Vec<(u64, u64)>>>;

fn len_of(body: &[u8]) -> u64 {
    body.len() as u64
}

fn sha256_of(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Collects every `(downloaded, total)` the downloader reports.
fn recording_progress() -> (ProgressCallback, ProgressLog) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let cb: ProgressCallback = Arc::new(move |d, t| sink.lock().unwrap().push((d, t)));
    (cb, seen)
}

struct Fixture {
    _dir: tempfile::TempDir,
    dest: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("create temp dir");
        let dest = dir.path().join("model.gguf");
        Self { _dir: dir, dest }
    }

    fn part(&self) -> PathBuf {
        part_path_for(&self.dest)
    }
}

fn request<'a>(
    url: &'a str,
    dest: &'a Path,
    expected_size: Option<u64>,
    progress: Option<ProgressCallback>,
) -> NativeDownload<'a> {
    NativeDownload {
        url,
        dest,
        token: None,
        expected_size,
        progress,
        cancel: None,
    }
}

// ============================================================================
// Happy path and atomicity
// ============================================================================

#[tokio::test]
async fn downloads_a_file_and_publishes_it_atomically() {
    let body = body_of(4096);
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    }])
    .await;
    let fx = Fixture::new();
    let (progress, seen) = recording_progress();

    let url = server.url();
    download_file(
        &Client::new(),
        &request(&url, &fx.dest, Some(len_of(&body)), Some(progress)),
    )
    .await
    .expect("download succeeds");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert!(
        !fx.part().exists(),
        "the .part file must be gone once the file is published"
    );

    let (last_downloaded, last_total) = {
        let seen = seen.lock().unwrap();
        *seen.last().expect("at least one progress report")
    };
    assert_eq!(last_downloaded, len_of(&body));
    assert_eq!(last_total, len_of(&body));
}

#[tokio::test]
async fn a_file_with_no_usable_etag_is_still_verified_by_size() {
    let body = body_of(2048);
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        // Opaque etag: not a digest, so it must be ignored rather than compared.
        etag: Some("686897696a7c876b7e".to_string()),
    }])
    .await;
    let fx = Fixture::new();

    let url = server.url();
    let err = download_file(&Client::new(), &request(&url, &fx.dest, Some(9999), None))
        .await
        .expect_err("a wrong expected size must fail");

    assert!(
        matches!(err, NativeError::SizeMismatch { .. }),
        "got {err:?}"
    );
    assert!(!fx.dest.exists());
}

// ============================================================================
// Resume after interrupt
// ============================================================================

#[tokio::test]
async fn resumes_after_an_interrupted_transfer() {
    let body = body_of(8192);
    let cut = 3000;
    let etag = sha256_of(&body);

    let server = TestServer::start(vec![
        Reply::Truncated {
            body: body.clone(),
            limit: cut,
        },
        Reply::File {
            body: body.clone(),
            etag: Some(etag),
        },
    ])
    .await;
    let fx = Fixture::new();
    let url = server.url();
    let size = Some(len_of(&body));

    // First attempt: the server hangs up mid-body.
    let err = download_file(&Client::new(), &request(&url, &fx.dest, size, None))
        .await
        .expect_err("a truncated body must not be accepted");
    assert!(
        matches!(
            err,
            NativeError::Network(_) | NativeError::SizeMismatch { .. }
        ),
        "got {err:?}"
    );

    // The bytes that did arrive are kept for the retry.
    let partial = std::fs::metadata(fx.part())
        .expect("a .part file survives the interruption")
        .len();
    let sent = len_of(&body[..cut]);
    assert!(
        partial > 0 && partial <= sent,
        "kept {partial} of the {sent} bytes that arrived"
    );
    assert!(!fx.dest.exists(), "nothing is published yet");

    // Second attempt: resumes rather than restarting.
    let (progress, seen) = recording_progress();
    download_file(
        &Client::new(),
        &request(&url, &fx.dest, size, Some(progress)),
    )
    .await
    .expect("the retry completes");

    assert_eq!(
        std::fs::read(&fx.dest).unwrap(),
        body,
        "the resumed file must be byte-exact — proving the hasher was seeded \
         with the bytes already on disk, since the etag check passed"
    );
    assert!(!fx.part().exists());

    let ranges = server.ranges();
    assert_eq!(ranges.len(), 2, "two requests: {ranges:?}");
    assert_eq!(ranges[0], None, "the first attempt asks for the whole file");
    assert_eq!(
        ranges[1],
        Some(format!("bytes={partial}-")),
        "the retry asks only for what is missing"
    );

    let reports = seen.lock().unwrap().clone();
    assert_eq!(
        reports.first().map(|(d, _)| *d),
        Some(partial),
        "progress resumes from the bytes already on disk, it does not restart at 0"
    );
    assert!(
        reports.windows(2).all(|w| w[1].0 >= w[0].0),
        "progress must never go backwards: {reports:?}"
    );
}

#[tokio::test]
async fn restarts_when_the_server_ignores_the_range_request() {
    let body = body_of(5000);
    let fx = Fixture::new();

    // Leave a stale partial behind that does NOT match the real file.
    std::fs::write(fx.part(), vec![0xff_u8; 1000]).unwrap();

    let server = TestServer::start(vec![Reply::IgnoresRange { body: body.clone() }]).await;
    let url = server.url();

    download_file(
        &Client::new(),
        &request(&url, &fx.dest, Some(len_of(&body)), None),
    )
    .await
    .expect("download succeeds");

    assert_eq!(
        std::fs::read(&fx.dest).unwrap(),
        body,
        "a 200 answer to a ranged request means the stale partial must be discarded"
    );
}

#[tokio::test]
async fn a_complete_partial_file_is_published_without_refetching() {
    let body = body_of(2048);
    let fx = Fixture::new();
    std::fs::write(fx.part(), &body).unwrap();

    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    }])
    .await;
    let url = server.url();

    download_file(
        &Client::new(),
        &request(&url, &fx.dest, Some(len_of(&body)), None),
    )
    .await
    .expect("a 416 means the file is already complete");

    assert_eq!(std::fs::read(&fx.dest).unwrap(), body);
    assert!(!fx.part().exists());
}

// ============================================================================
// Checksum mismatch
// ============================================================================

#[tokio::test]
async fn a_checksum_mismatch_fails_and_clears_the_partial_file() {
    let body = body_of(4096);
    let server = TestServer::start(vec![Reply::File {
        body,
        // A valid-looking digest for entirely different content.
        etag: Some(sha256_of(b"something else")),
    }])
    .await;
    let fx = Fixture::new();

    let url = server.url();
    let err = download_file(&Client::new(), &request(&url, &fx.dest, None, None))
        .await
        .expect_err("bytes that do not match the advertised digest must be rejected");

    match err {
        NativeError::ChecksumMismatch { expected, actual } => {
            assert_ne!(expected, actual);
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }

    assert!(!fx.dest.exists(), "corrupt bytes must never be published");
    assert!(
        !fx.part().exists(),
        "the .part file must be cleared so a retry starts clean rather than \
         resuming onto known-bad bytes forever"
    );
}

// ============================================================================
// Network failure
// ============================================================================

#[tokio::test]
async fn a_404_is_reported_as_not_found() {
    let server = TestServer::start(vec![Reply::Status(404)]).await;
    let fx = Fixture::new();

    let url = server.url();
    let err = download_file(&Client::new(), &request(&url, &fx.dest, None, None))
        .await
        .expect_err("404 must fail");

    assert!(matches!(err, NativeError::NotFound(_)), "got {err:?}");
    assert!(!fx.dest.exists());
    assert!(!fx.part().exists(), "nothing should be created for a 404");
}

#[tokio::test]
async fn a_server_error_preserves_its_status_code() {
    let server = TestServer::start(vec![Reply::Status(503)]).await;
    let fx = Fixture::new();

    let url = server.url();
    let err = download_file(&Client::new(), &request(&url, &fx.dest, None, None))
        .await
        .expect_err("503 must fail");

    match err {
        NativeError::Http { status, .. } => assert_eq!(status, 503),
        other => panic!("expected Http, got {other:?}"),
    }
    assert!(!fx.dest.exists());
}

#[tokio::test]
async fn an_unreachable_host_is_a_network_error() {
    // Bind and immediately drop, so the port is almost certainly refused.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let fx = Fixture::new();
    let url = format!("http://{addr}/model.gguf");
    let err = download_file(&Client::new(), &request(&url, &fx.dest, None, None))
        .await
        .expect_err("a refused connection must fail");

    assert!(matches!(err, NativeError::Network(_)), "got {err:?}");
    assert!(!fx.dest.exists());
}

#[tokio::test]
async fn a_cancelled_download_keeps_its_partial_file() {
    let body = body_of(8192);
    let server = TestServer::start(vec![Reply::Truncated { body, limit: 2000 }]).await;
    let fx = Fixture::new();

    let cancel = CancellationToken::new();
    cancel.cancel();

    let url = server.url();
    let err = download_file(
        &Client::new(),
        &NativeDownload {
            url: &url,
            dest: &fx.dest,
            token: None,
            expected_size: None,
            progress: None,
            cancel: Some(cancel),
        },
    )
    .await
    .expect_err("a cancelled download must not report success");

    assert!(matches!(err, NativeError::Cancelled), "got {err:?}");
    assert!(!fx.dest.exists());
    assert_eq!(server.request_count(), 1);
}

// ============================================================================
// Header parsing
// ============================================================================

#[test]
fn expected_digest_prefers_the_linked_etag() {
    let digest = "a".repeat(64);
    let other = "b".repeat(64);
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, format!("\"{other}\"").parse().unwrap());
    headers.insert(X_LINKED_ETAG, format!("\"{digest}\"").parse().unwrap());

    assert_eq!(expected_digest(&headers), Some(digest));
}

#[test]
fn expected_digest_falls_back_to_the_plain_etag() {
    let digest = "c".repeat(64);
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, format!("W/\"{digest}\"").parse().unwrap());

    assert_eq!(expected_digest(&headers), Some(digest));
}

#[test]
fn a_non_digest_etag_is_not_treated_as_a_checksum() {
    // Real HuggingFace responses carry opaque etags for non-LFS files. Comparing
    // a SHA-256 against one of those would fail every download.
    for value in [
        "\"686897696a7c876b7e\"",
        "\"not-a-hash\"",
        "\"\"",
        "W/\"xyz\"",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(ETAG, value.parse().unwrap());
        assert_eq!(expected_digest(&headers), None, "for etag {value}");
    }
}

#[test]
fn an_absent_etag_yields_no_digest() {
    assert_eq!(expected_digest(&HeaderMap::new()), None);
}

#[test]
fn content_range_supplies_the_authoritative_total() {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_RANGE, "bytes 100-199/500".parse().unwrap());
    headers.insert(reqwest::header::CONTENT_LENGTH, "100".parse().unwrap());

    assert_eq!(total_size(&headers, 100), Some(500));
}

#[test]
fn content_length_plus_the_resume_offset_is_the_fallback_total() {
    let mut headers = HeaderMap::new();
    headers.insert(reqwest::header::CONTENT_LENGTH, "400".parse().unwrap());

    assert_eq!(total_size(&headers, 100), Some(500));
}

#[test]
fn an_unknown_content_range_total_is_not_invented() {
    assert_eq!(parse_content_range_total("bytes 0-99/*"), None);
    assert_eq!(parse_content_range_total("bytes 0-99/500"), Some(500));
}

#[test]
fn part_paths_sit_beside_their_destination() {
    let dest = Path::new("/models/repo/model.gguf");
    assert_eq!(
        part_path_for(dest),
        PathBuf::from("/models/repo/model.gguf.part")
    );
}
