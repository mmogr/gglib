//! Tests for the repair of a model with a file whose role cannot be told:
//! its header is unreadable, and the model links it as nothing. Such a file
//! is deleted, and named as coming back, only when the download fetches it.

use super::tests::{PROJECTOR, WEIGHTS, fixture, row, sha256};
use crate::domain::ModelFile;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};

/// A file the model's download does not fetch, beside its weights.
const STRAY: &str = "zeta-extra.gguf";

/// A repository whose variant is [`WEIGHTS`], with [`PROJECTOR`] beside it:
/// a download fetches those two and nothing else.
fn hub() -> FakeHub {
    FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        projectors: vec![hub_file(PROJECTOR, 9, "p-oid")],
        ..Default::default()
    }
}

/// The rows for corrupt [`WEIGHTS`] in `dir` and [`STRAY`], whose bytes
/// (when `stray` holds some) are no header and no match for its row.
fn corrupt_weights_and_stray(dir: &std::path::Path, stray: Option<&str>) -> Vec<ModelFile> {
    std::fs::write(dir.join(WEIGHTS), "damaged").unwrap();
    if let Some(bytes) = stray {
        std::fs::write(dir.join(STRAY), bytes).unwrap();
    }
    vec![
        row(0, WEIGHTS, &sha256("weights")),
        row(1, STRAY, &sha256("weights")),
    ]
}

/// Missing, or there with a header that cannot be read: either way the
/// weights are repaired, and the stray file is neither deleted nor named as
/// coming back.
#[tokio::test]
async fn an_unknown_file_no_download_fetches_is_left_and_not_named() {
    for stray in [None, Some("damaged")] {
        let dir = tempfile::tempdir().unwrap();
        let rows = corrupt_weights_and_stray(dir.path(), stray);
        let f = fixture(dir.path(), rows, hub());

        let started = f.service.repair_model(1, None).await.unwrap();

        assert_eq!(started.files, [WEIGHTS], "{stray:?}");
        assert!(!dir.path().join(WEIGHTS).exists(), "{stray:?}");
        assert_eq!(
            dir.path().join(STRAY).exists(),
            stray.is_some(),
            "{stray:?}: left as it was"
        );
        assert_eq!(f.queued.asked().len(), 1, "{stray:?}");
    }
}

/// When the only unhealthy file is one no download fetches, it is named,
/// nothing is deleted and nothing is queued. It is no projector, so no
/// other is offered.
#[tokio::test]
async fn an_unknown_file_alone_is_named_and_left_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let rows = corrupt_weights_and_stray(dir.path(), Some("damaged"));
    std::fs::write(dir.path().join(WEIGHTS), "weights").unwrap();
    let f = fixture(dir.path(), rows, hub());

    let refused = f.service.repair_model(1, None).await.unwrap_err();

    assert!(refused.contains(STRAY), "{refused}");
    assert!(refused.ends_with("so it was left in place"), "{refused}");
    assert!(dir.path().join(STRAY).exists(), "nothing is deleted");
    assert!(f.queued.asked().is_empty(), "nothing is queued");
}

/// A file whose role cannot be told that the download does fetch, a shard
/// of the variant, is repaired as one.
#[tokio::test]
async fn an_unknown_file_the_download_fetches_is_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let rows = corrupt_weights_and_stray(dir.path(), Some("damaged"));
    let mut hub = hub();
    hub.weights.push(hub_file(STRAY, 7, "s-oid"));
    let f = fixture(dir.path(), rows, hub);

    let started = f.service.repair_model(1, None).await.unwrap();

    assert_eq!(started.files, [WEIGHTS, STRAY]);
    assert!(!dir.path().join(STRAY).exists());
}
