//! Tests for the download a repair queues: the quantization it asks for,
//! what it answers, and what it says when the files are gone and nothing
//! fetches them.

use std::path::Path;
use std::sync::Arc;

use super::tests::{Fixture, REPO, Rows, WEIGHTS, fixture, fixture_queueing_on, row, sha256};
use crate::domain::ModelFile;
use crate::download::{DownloadError, DownloadId};
use crate::ports::AskedDownloads;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};
use crate::services::{RepairStarted, missing_after_repair};

/// A repository holding [`WEIGHTS`].
fn hub() -> FakeHub {
    FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        ..Default::default()
    }
}

/// [`WEIGHTS`] in `dir` with bytes its row does not match, and that row.
fn corrupt_weights(dir: &Path) -> Vec<ModelFile> {
    std::fs::write(dir.join(WEIGHTS), "damaged").unwrap();
    vec![row(0, WEIGHTS, &sha256("weights"))]
}

/// A model stored as `quantization` with corrupt weights in `dir`, whose
/// repair queues on `queued`.
fn stored_as(dir: &Path, quantization: &str, queued: AskedDownloads) -> Fixture {
    let rows = Arc::new(Rows(corrupt_weights(dir)));
    fixture_queueing_on(dir, quantization, rows, hub(), queued)
}

// ── The quantization asked for ───────────────────────────────────────────

/// The shapes a quantization is stored in, each with the one a download asks
/// the Hub and the queue for. An Unsloth Dynamic quantization stays one, and
/// none of them becomes a default.
#[tokio::test]
async fn a_repair_asks_for_the_quantization_the_model_was_stored_with() {
    let stored = [
        ("Q8_0", "Q8_0"),
        ("q4_k_m", "Q4_K_M"),
        ("Q6_K", "Q6_K"),
        ("UD-Q6_K", "UD-Q6_K"),
        ("ud-q6_k", "UD-Q6_K"),
        ("UD_Q4_K_M", "UD-Q4_K_M"),
        ("F16", "F16"),
        ("FP16", "F16"),
        ("BF16", "BF16"),
        ("IQ4_XS", "IQ4_XS"),
        ("imatrix", "imatrix"),
    ];
    for (stored, asked) in stored {
        let dir = tempfile::tempdir().unwrap();
        let f = stored_as(dir.path(), stored, AskedDownloads::default());

        let started = f.service.repair_model(1, None).await.unwrap();

        assert_eq!(
            *f.hub.quantizations_asked.lock().unwrap(),
            [asked],
            "the Hub, for a model stored as {stored}"
        );
        assert_eq!(
            f.queued.asked(),
            [(REPO.to_owned(), Some(asked.to_owned()))],
            "the queue, for a model stored as {stored}"
        );
        assert_eq!(started.id, format!("{REPO}:{asked}"));
    }
}

/// A stored quantization that names none is refused while the file is still
/// there: nothing stands in for it.
#[tokio::test]
async fn a_stored_quantization_no_download_asks_for_is_refused_before_anything_is_deleted() {
    for stored in ["BANANA", "Unknown", "Q4_K_M_typo", ""] {
        let dir = tempfile::tempdir().unwrap();
        let f = stored_as(dir.path(), stored, AskedDownloads::default());

        let refused = f.service.repair_model(1, None).await.unwrap_err();

        assert_eq!(
            refused,
            format!("{stored} is not a quantization a download asks for")
        );
        assert!(dir.path().join(WEIGHTS).exists(), "nothing is deleted");
        assert!(f.queued.asked().is_empty(), "nothing is queued");
        assert!(f.hub.quantizations_asked.lock().unwrap().is_empty());
    }
}

// ── What a repair answers ────────────────────────────────────────────────

/// Three shards: one corrupt, one missing, one healthy. The answer names the
/// download and the two it is to bring back, and the healthy one stays.
#[tokio::test]
async fn a_repair_answers_its_download_and_the_files_it_fetches_again() {
    let (corrupt, missing, healthy) = ("z-00001.gguf", "z-00002.gguf", "z-00003.gguf");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(corrupt), "damaged").unwrap();
    std::fs::write(dir.path().join(healthy), "weights").unwrap();
    let rows = vec![
        row(0, corrupt, &sha256("weights")),
        row(1, missing, &sha256("weights")),
        row(2, healthy, &sha256("weights")),
    ];
    let f = fixture(dir.path(), rows, hub());

    let started = f.service.repair_model(1, None).await.unwrap();

    assert_eq!(
        started,
        RepairStarted {
            id: "owner/zeta-GGUF:Q8_0".to_owned(),
            files: vec![corrupt.to_owned(), missing.to_owned()],
        }
    );
    assert!(!dir.path().join(corrupt).exists(), "the corrupt file goes");
    assert!(dir.path().join(healthy).exists(), "the healthy one stays");
    assert_eq!(f.queued.asked().len(), 1, "one download for both");
}

// ── When nothing fetches the files ───────────────────────────────────────

/// The Hub is asked what the download fetches before any file is deleted, so
/// a repository that does not answer costs the model nothing.
#[tokio::test]
async fn a_repository_that_cannot_be_read_stops_a_repair_before_anything_is_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let rows = corrupt_weights(dir.path());
    let f = fixture(dir.path(), rows, FakeHub::default());

    let failed = f.service.repair_model(1, None).await.unwrap_err();

    assert!(
        failed.starts_with("Failed to read what a download of owner/zeta-GGUF fetches: "),
        "{failed}"
    );
    assert!(dir.path().join(WEIGHTS).exists(), "nothing is deleted");
    assert!(f.queued.asked().is_empty(), "nothing is queued");
}

/// The queue refuses once the file is gone. The repair fails, and says which
/// file is missing and the command that fetches it.
#[tokio::test]
async fn a_queue_that_refuses_after_the_delete_names_the_missing_files_and_what_fetches_them() {
    let dir = tempfile::tempdir().unwrap();
    let full = AskedDownloads::refusing(DownloadError::queue_full(10));
    let f = stored_as(dir.path(), "Q8_0", full);

    let failed = f.service.repair_model(1, None).await.unwrap_err();

    assert_eq!(
        failed,
        "The download that fetches them again could not be queued: Queue full: maximum 10 \
         downloads allowed. Missing from the model's folder: zeta.Q8_0.gguf. Run `gglib model \
         download owner/zeta-GGUF --quantization Q8_0` to fetch what is missing."
    );
    assert!(!dir.path().join(WEIGHTS).exists(), "as the message says");
    assert_eq!(f.queued.asked().len(), 1, "it was asked once");
}

/// A directory where a shard should be cannot be deleted as a file. It is
/// not fetched again, so it is not named, and the shard that could be
/// deleted is.
#[tokio::test]
async fn a_file_that_cannot_be_deleted_is_not_among_those_fetched_again() {
    let (stuck, corrupt) = ("z-00001.gguf", "z-00002.gguf");
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(stuck)).unwrap();
    std::fs::write(dir.path().join(corrupt), "damaged").unwrap();
    let rows = vec![
        row(0, stuck, &sha256("weights")),
        row(1, corrupt, &sha256("weights")),
    ];
    let f = fixture(dir.path(), rows, hub());

    let started = f.service.repair_model(1, None).await.unwrap();

    assert_eq!(started.files, [corrupt]);
    assert!(dir.path().join(stuck).exists());
}

/// When none of the unhealthy files can be deleted, a download would fetch
/// nothing: the repair fails and queues none.
#[tokio::test]
async fn a_repair_that_could_delete_nothing_queues_nothing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(WEIGHTS)).unwrap();
    let rows = vec![row(0, WEIGHTS, &sha256("weights"))];
    let f = fixture(dir.path(), rows, hub());

    let failed = f.service.repair_model(1, None).await.unwrap_err();

    assert_eq!(
        failed,
        "No unhealthy file could be deleted, so nothing was queued"
    );
    assert!(f.queued.asked().is_empty());
}

// ── The words for files that did not come back ───────────────────────────

#[test]
fn the_missing_files_are_named_with_the_command_that_fetches_them() {
    let files = ["a.gguf".to_owned(), "b.gguf".to_owned()];

    assert_eq!(
        missing_after_repair(&DownloadId::new("owner/repo", Some("UD-Q6_K")), &files),
        "Missing from the model's folder: a.gguf, b.gguf. Run `gglib model download owner/repo \
         --quantization UD-Q6_K` to fetch what is missing."
    );
    assert_eq!(
        missing_after_repair(&DownloadId::from("owner/repo"), &files[..1]),
        "Missing from the model's folder: a.gguf. Run `gglib model download owner/repo` to \
         fetch what is missing."
    );
}
