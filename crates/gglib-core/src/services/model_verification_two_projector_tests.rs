//! Tests for the repair of a model that has two projector rows: each is
//! deleted, or left, on its own.

use super::tests::{PROJECTOR, WEIGHTS, fixture, row, sha256};
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};

/// Two corrupt projectors, as the older grouping stored them: both carry the
/// weights' quantization. The download fetches the first by name, so that
/// one goes and the other, which nothing would bring back, stays.
#[tokio::test]
async fn of_two_corrupt_projectors_only_the_fetched_one_is_deleted() {
    let (fetched, other) = ("a.mmproj-Q8_0.gguf", "b.mmproj-Q8_0.gguf");
    let dir = tempfile::tempdir().unwrap();
    for name in [WEIGHTS, fetched, other] {
        std::fs::write(dir.path().join(name), "damaged").unwrap();
    }
    let rows = vec![
        row(0, WEIGHTS, &sha256("damaged")),
        row(1, fetched, &sha256("projector")),
        row(2, other, &sha256("projector")),
    ];
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        projectors: vec![hub_file(fetched, 9, "a-oid"), hub_file(other, 9, "b-oid")],
        ..Default::default()
    };
    let f = fixture(dir.path(), rows, hub);

    f.service.repair_model(1, None).await.unwrap();

    assert!(!dir.path().join(fetched).exists(), "it is fetched again");
    assert!(dir.path().join(other).exists(), "nothing brings it back");
    assert!(dir.path().join(WEIGHTS).exists(), "healthy weights stay");
    assert_eq!(f.queued.0.lock().unwrap().len(), 1);
}

/// The only unhealthy files are projectors no download fetches: each is
/// named, none is deleted and nothing is queued.
#[tokio::test]
async fn projectors_no_download_fetches_are_all_named_and_left_in_place() {
    let (first, second) = ("a.mmproj-Q8_0.gguf", "b.mmproj-Q8_0.gguf");
    let dir = tempfile::tempdir().unwrap();
    for name in [first, second] {
        std::fs::write(dir.path().join(name), "damaged").unwrap();
    }
    let rows = vec![
        row(1, first, &sha256("projector")),
        row(2, second, &sha256("projector")),
    ];
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        projectors: vec![hub_file(PROJECTOR, 9, "p-oid")],
        ..Default::default()
    };
    let f = fixture(dir.path(), rows, hub);

    let refused = f.service.repair_model(1, None).await.unwrap_err();

    assert!(
        refused.contains(first) && refused.contains(second),
        "{refused}"
    );
    assert!(dir.path().join(first).exists() && dir.path().join(second).exists());
    assert!(f.queued.0.lock().unwrap().is_empty(), "nothing is queued");
}
