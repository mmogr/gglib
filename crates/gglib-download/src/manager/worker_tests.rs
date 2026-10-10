//! Tests of the download worker.

use super::*;

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
    };
    let deps = WorkerDeps {
        config: DownloadManagerConfig::default(),
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
