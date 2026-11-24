//! Tests of the download worker.

use std::time::Duration;

use async_trait::async_trait;
use gglib_core::ports::huggingface::HfPortResult;
use gglib_core::ports::huggingface::fake_hub::FakeHub;
use gglib_core::ports::{HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult};

use super::*;
use crate::executor::native::tests::{Reply, TestServer, body_of, sha256_of};

const fn at(bytes: u64, wire: u64) -> FileProgress {
    FileProgress {
        bytes,
        wire,
        size: None,
    }
}

#[test]
fn a_notice_clears_on_the_next_advance() {
    let mut update = ProgressUpdate::default();
    update.advance(at(500, 500));
    update.notice = Some("using direct transfer…".to_string());

    // The restart the notice announced, then a reading with nothing new.
    update.advance(at(0, 500));
    update.advance(at(0, 500));
    assert!(update.notice.is_some(), "nothing has arrived yet");

    update.advance(at(0, 600));
    assert_eq!(update.notice, None, "network bytes are progress");

    update.notice = Some("again".to_string());
    update.advance(at(10, 600));
    assert_eq!(update.notice, None, "and so are bytes on disk");
}

/// A companion's file is fetched from its own repository, into its job's
/// destination, though its download is another repository's.
#[test]
fn a_jobs_file_is_fetched_from_the_repository_it_comes_from() {
    let (progress_tx, _) = watch::channel(ProgressUpdate::default());
    let job = DownloadJob {
        id: DownloadId::new("leejet/FLUX.1-schnell-gguf", Some("Q8_0")),
        repo: "unsloth/FLUX.1-schnell".to_string(),
        destination: DownloadDestination::plan(
            std::path::Path::new("/models"),
            "unsloth/FLUX.1-schnell",
            vec!["ae.safetensors".to_string()],
        ),
        revision: None,
        cancel: CancellationToken::new(),
        progress_tx,
        expected_size: Some(10),
        upgrade: None,
    };
    let deps = WorkerDeps {
        config: DownloadManagerConfig::default(),
        hub: Arc::new(FakeHub::default()),
    };

    let plan = plan_for(
        &job,
        "ae.safetensors",
        &deps,
        Arc::new(|_| {}),
        Arc::new(|_| {}),
    );

    assert_eq!(plan.repo_id, "unsloth/FLUX.1-schnell");
    assert_eq!(
        plan.destination,
        std::path::Path::new("/models/unsloth_FLUX.1-schnell")
    );
    assert_eq!(plan.file, "ae.safetensors");
    assert_eq!(plan.revision, "main");
}

#[test]
fn test_percent_encode_revision() {
    // Normal alphanumeric revisions pass through
    assert_eq!(percent_encode_revision("main"), "main");
    assert_eq!(percent_encode_revision("v1.0.2"), "v1.0.2");
    assert_eq!(percent_encode_revision("abc123-def"), "abc123-def");

    // Branch names with slashes
    assert_eq!(
        percent_encode_revision("feature/branch"),
        "feature%2Fbranch"
    );
    assert_eq!(percent_encode_revision("hotfix/v1.2"), "hotfix%2Fv1.2");

    // Special characters that could cause ambiguity
    assert_eq!(percent_encode_revision("tag#123"), "tag%23123");
    assert_eq!(percent_encode_revision("user@commit"), "user%40commit");

    // Complex case
    assert_eq!(
        percent_encode_revision("feature/test@v1#fix"),
        "feature%2Ftest%40v1%23fix"
    );

    // Unicode (UTF-8 encoding)
    let encoded = percent_encode_revision("café/模型#x@y");
    assert!(encoded.contains("%C3%A9"), "Should contain UTF-8 encoded é");
    assert!(encoded.contains("%2F"), "Should encode /");
    assert!(encoded.contains("%23"), "Should encode #");
    assert!(encoded.contains("%40"), "Should encode @");
    // Verify it encodes the CJK character (模 = E6 A8 A1 in UTF-8)
    assert!(
        encoded.contains("%E6%A8%A1"),
        "Should contain UTF-8 encoded 模"
    );

    // Full verification
    assert_eq!(percent_encode_revision("café"), "caf%C3%A9");
}

/// A Hub that sends every file's download to one address. The worker asks
/// it nothing else.
struct ServedAt(String);

#[async_trait]
impl HfClientPort for ServedAt {
    async fn search(&self, _: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unreachable!("the worker fetches by address alone")
    }
    async fn list_quantizations(&self, _: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        unreachable!("the worker fetches by address alone")
    }
    async fn list_gguf_files(&self, _: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unreachable!("the worker fetches by address alone")
    }
    async fn get_quantization_files(&self, _: &str, _: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unreachable!("the worker fetches by address alone")
    }
    async fn get_commit_sha(&self, _: &str) -> HfPortResult<String> {
        unreachable!("the worker fetches by address alone")
    }
    async fn get_model_info(&self, _: &str) -> HfPortResult<HfRepoInfo> {
        unreachable!("the worker fetches by address alone")
    }
    async fn read_head(&self, _: &str, _: &str, _: u64) -> HfPortResult<Vec<u8>> {
        unreachable!("the worker fetches by address alone")
    }
    fn file_url(&self, _: &str, _: &str, _: &str) -> String {
        self.0.clone()
    }
    async fn file_at(&self, _: &str, _: &str) -> HfPortResult<Option<HfFileInfo>> {
        unreachable!("the worker fetches by address alone")
    }
}

/// A cancel is answered once the bytes that arrived are in the `.part`
/// file, so what the manager announces as cancelled is already resumable.
/// The transfer's sink buffers megabytes before it writes, and only a
/// closed channel makes it write; a worker that dropped the transfer on the
/// token would answer while that write is still to come.
#[tokio::test]
async fn a_cancel_is_answered_once_the_part_holds_what_arrived() {
    gglib_core::paths::isolate_data_root();
    let body = body_of(8192);
    let sent = 3000;
    let server = TestServer::start(vec![Reply::Stalls { body, limit: sent }]).await;
    let models = tempfile::tempdir().unwrap();

    let (progress_tx, mut progress_rx) = watch::channel(ProgressUpdate::default());
    let job = DownloadJob {
        id: DownloadId::new("tester/stalled-GGUF", Some("Q4_0")),
        repo: "tester/stalled-GGUF".to_string(),
        destination: DownloadDestination::plan(
            models.path(),
            "tester/stalled-GGUF",
            vec!["stalled.gguf".to_string()],
        ),
        revision: None,
        cancel: CancellationToken::new(),
        progress_tx,
        expected_size: None,
        upgrade: None,
    };
    let deps = WorkerDeps {
        config: DownloadManagerConfig::default(),
        hub: Arc::new(ServedAt(server.url())),
    };

    let fetch = execute_download(&job, &deps);
    let cancel_once_bytes_arrive = async {
        progress_rx
            .wait_for(|update| update.progress.wire > 0)
            .await
            .expect("the worker reports the first bytes");
        // Long enough for the rest of what was sent to arrive on loopback.
        tokio::time::sleep(Duration::from_millis(250)).await;
        job.cancel.cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(fetch, cancel_once_bytes_arrive)
    })
    .await
    .expect("the cancelled fetch ends");

    assert!(
        matches!(result, Err(DownloadError::Cancelled)),
        "got {result:?}"
    );
    let part = job.destination.model_dir.join("stalled.gguf.part");
    let kept = std::fs::metadata(&part).map_or(0, |m| m.len());
    assert_eq!(
        kept, sent as u64,
        "the .part holds what arrived when the cancel is answered"
    );
}

/// A download that ended with its `.part` on disk (it gave up on a quiet
/// connection, or failed, or was cancelled) is queued again: its job asks
/// only for what is missing, so trying again carries on rather than
/// starting over.
#[tokio::test]
async fn a_job_run_again_carries_on_from_the_part() {
    gglib_core::paths::isolate_data_root();
    let body = body_of(8192);
    let kept = 3000;
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    }])
    .await;
    let models = tempfile::tempdir().unwrap();

    let (progress_tx, _progress_rx) = watch::channel(ProgressUpdate::default());
    let job = DownloadJob {
        id: DownloadId::new("tester/again-GGUF", Some("Q4_0")),
        repo: "tester/again-GGUF".to_string(),
        destination: DownloadDestination::plan(
            models.path(),
            "tester/again-GGUF",
            vec!["again.gguf".to_string()],
        ),
        revision: None,
        cancel: CancellationToken::new(),
        progress_tx,
        expected_size: Some(body.len() as u64),
        upgrade: None,
    };
    job.destination.ensure_dir().unwrap();
    let part = job.destination.model_dir.join("again.gguf.part");
    std::fs::write(&part, &body[..kept]).unwrap();
    let deps = WorkerDeps {
        config: DownloadManagerConfig::default(),
        hub: Arc::new(ServedAt(server.url())),
    };

    tokio::time::timeout(Duration::from_secs(10), execute_download(&job, &deps))
        .await
        .expect("the fetch ends")
        .expect("the fetch completes");

    assert_eq!(server.ranges(), [Some(format!("bytes={kept}-"))]);
    assert_eq!(
        std::fs::read(job.destination.model_dir.join("again.gguf")).unwrap(),
        body
    );
}

/// An upgrade's file is fetched again over the copy on disk, pinned to its
/// commit. An upgrade tried again at the same commit carries on from the
/// part that commit's earlier run left; a part of any other revision goes,
/// and the old copy stays until the new one replaces it.
#[tokio::test]
async fn an_upgrade_run_again_carries_on_from_its_own_commits_part() {
    gglib_core::paths::isolate_data_root();
    let body = body_of(8192);
    let kept = 3000;
    let commit = "2222222222222222222222222222222222222222";
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    }])
    .await;
    let models = tempfile::tempdir().unwrap();

    let (progress_tx, _progress_rx) = watch::channel(ProgressUpdate::default());
    let job = DownloadJob {
        id: DownloadId::new("tester/upgrade-GGUF", Some("Q4_0")),
        repo: "tester/upgrade-GGUF".to_string(),
        destination: DownloadDestination::plan(
            models.path(),
            "tester/upgrade-GGUF",
            vec!["up.gguf".to_string()],
        ),
        revision: None,
        cancel: CancellationToken::new(),
        progress_tx,
        expected_size: Some(body.len() as u64),
        upgrade: Some(crate::queue::Upgrade {
            model_id: 7,
            commit: commit.to_string(),
        }),
    };
    job.destination.ensure_dir().unwrap();
    let dir = job.destination.model_dir.clone();
    std::fs::write(dir.join("up.gguf"), b"the old revision").unwrap();
    std::fs::write(dir.join(format!("up.gguf.{commit}.part")), &body[..kept]).unwrap();
    std::fs::write(dir.join("up.gguf.part"), b"another revision's start").unwrap();
    std::fs::write(dir.join("up.gguf.1111.part"), b"an older upgrade's start").unwrap();
    let deps = WorkerDeps {
        config: DownloadManagerConfig::default(),
        hub: Arc::new(ServedAt(server.url())),
    };

    tokio::time::timeout(Duration::from_secs(10), execute_download(&job, &deps))
        .await
        .expect("the fetch ends")
        .expect("the fetch completes");

    assert_eq!(server.ranges(), [Some(format!("bytes={kept}-"))]);
    assert_eq!(std::fs::read(dir.join("up.gguf")).unwrap(), body);
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, ["up.gguf"], "no part of any revision is left");
}

/// A download the Hub refused for its rate limit shows the wait on its row,
/// and the row drops it once the file's bytes arrive after the wait.
#[tokio::test]
async fn a_rate_limit_wait_is_shown_on_the_row_until_bytes_arrive() {
    gglib_core::paths::isolate_data_root();
    let body = body_of(4096);
    let server = TestServer::start(vec![
        Reply::Limited { t: 1 },
        Reply::File {
            body: body.clone(),
            etag: Some(sha256_of(&body)),
        },
    ])
    .await;
    let models = tempfile::tempdir().unwrap();

    let (progress_tx, mut progress_rx) = watch::channel(ProgressUpdate::default());
    let job = DownloadJob {
        id: DownloadId::new("tester/limited-GGUF", Some("Q4_0")),
        repo: "tester/limited-GGUF".to_string(),
        destination: DownloadDestination::plan(
            models.path(),
            "tester/limited-GGUF",
            vec!["limited.gguf".to_string()],
        ),
        revision: None,
        cancel: CancellationToken::new(),
        progress_tx,
        expected_size: Some(body.len() as u64),
        upgrade: None,
    };
    let deps = WorkerDeps {
        config: DownloadManagerConfig::default(),
        hub: Arc::new(ServedAt(server.resolve_url())),
    };
    let waiting = gglib_core::hub_limit::waiting_sentence(Duration::from_secs(1));
    let shown = async {
        progress_rx
            .wait_for(|update| update.notice.as_deref() == Some(waiting.as_str()))
            .await
            .is_ok()
    };

    let (fetched, shown) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(execute_download(&job, &deps), shown)
    })
    .await
    .expect("the fetch ends");

    fetched.expect("the fetch completes once the limit lifts");
    assert!(shown, "the row showed the wait");
    let last = progress_rx.borrow().clone();
    assert_eq!(last.notice, None, "the bytes that came after it cleared it");
    assert_eq!(server.request_count(), 2);
}

/// A job's transfer keeps the Hub's rate limit in the manager's gate: with
/// the file allowance already closed there, the row shows the wait and the
/// file is not asked for.
#[tokio::test]
async fn a_job_waits_on_the_managers_gate_before_asking() {
    use gglib_core::hub_limit::{HubBucket, HubGate, HubLimit};

    gglib_core::paths::isolate_data_root();
    let body = body_of(2048);
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: Some(sha256_of(&body)),
    }])
    .await;
    let models = tempfile::tempdir().unwrap();
    let gate = Arc::new(HubGate::new());
    gate.note(
        HubLimit {
            bucket: HubBucket::Resolvers,
            remaining: 0,
            resets_in: Duration::from_mins(2),
        },
        tokio::time::Instant::now(),
    );

    let (progress_tx, mut progress_rx) = watch::channel(ProgressUpdate::default());
    let job = DownloadJob {
        id: DownloadId::new("tester/gated-GGUF", Some("Q4_0")),
        repo: "tester/gated-GGUF".to_string(),
        destination: DownloadDestination::plan(
            models.path(),
            "tester/gated-GGUF",
            vec!["gated.gguf".to_string()],
        ),
        revision: None,
        cancel: CancellationToken::new(),
        progress_tx,
        expected_size: Some(body.len() as u64),
        upgrade: None,
    };
    let deps = WorkerDeps {
        config: DownloadManagerConfig {
            hub_gate: gate,
            ..DownloadManagerConfig::default()
        },
        hub: Arc::new(ServedAt(server.resolve_url())),
    };
    let cancel_once_shown = async {
        progress_rx
            .wait_for(|update| {
                update
                    .notice
                    .as_deref()
                    .is_some_and(|note| note.starts_with("waiting for Hugging Face's rate limit"))
            })
            .await
            .expect("the row shows the wait");
        let asked = server.request_count();
        job.cancel.cancel();
        asked
    };

    let (fetched, asked) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(execute_download(&job, &deps), cancel_once_shown)
    })
    .await
    .expect("the cancel ends the wait");

    assert_eq!(asked, 0, "the wait is shown before any request");
    assert!(
        matches!(fetched, Err(DownloadError::Cancelled)),
        "got {fetched:?}"
    );
    assert_eq!(server.request_count(), 0);
}
