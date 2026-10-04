//! Tests for [`download_group`].

use super::*;
use crate::ports::huggingface::HfPortError;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};

fn paths(group: &DownloadGroup) -> Vec<&str> {
    group.files().map(|f| f.path.as_str()).collect()
}

#[tokio::test]
async fn the_group_is_the_weights_then_the_chosen_projector() {
    let hub = FakeHub {
        weights: vec![
            hub_file("m-Q8_0-00001-of-00002.gguf", 10, "w1"),
            hub_file("m-Q8_0-00002-of-00002.gguf", 5, "w2"),
        ],
        projectors: vec![
            hub_file("mmproj-BF16.gguf", 3, "pb"),
            hub_file("mmproj-F16.gguf", 2, "pf"),
        ],
        ..Default::default()
    };

    let group = download_group(&hub, "o/r", Quantization::Q8_0)
        .await
        .unwrap();

    assert_eq!(
        paths(&group),
        [
            "m-Q8_0-00001-of-00002.gguf",
            "m-Q8_0-00002-of-00002.gguf",
            "mmproj-F16.gguf"
        ]
    );
    let projector = group.projector.expect("the F16 projector is chosen");
    assert_eq!(projector.size, 2);
    assert_eq!(projector.oid.as_deref(), Some("pf"));
}

#[tokio::test]
async fn a_repository_without_projectors_gives_the_weights_alone() {
    let hub = FakeHub {
        weights: vec![hub_file("m-Q8_0.gguf", 10, "w")],
        ..Default::default()
    };

    let group = download_group(&hub, "o/r", Quantization::Q8_0)
        .await
        .unwrap();

    assert_eq!(paths(&group), ["m-Q8_0.gguf"]);
    assert!(group.projector.is_none());
}

/// Projectors alone are not a download: the missing quantization is the
/// answer, not a group of one projector.
#[tokio::test]
async fn a_quantization_the_repository_lacks_is_not_found() {
    let hub = FakeHub {
        projectors: vec![hub_file("mmproj-F16.gguf", 2, "pf")],
        ..Default::default()
    };

    let refused = download_group(&hub, "o/r", Quantization::Q8_0).await;

    assert!(matches!(
        refused,
        Err(HfPortError::QuantizationNotFound { .. })
    ));
}

/// A listing names the file the download fetches: the projector of the same
/// quantization for one, the F16 one for another, with its size and OID.
#[test]
fn the_projector_fetched_with_a_quantization_is_the_chosen_one() {
    let projectors = [
        hub_file("X.mmproj-Q8_0.gguf", 3, "p8"),
        hub_file("mmproj-F16.gguf", 2, "pf"),
    ];

    let with_q8 = projector_fetched_with(Quantization::Q8_0, &projectors).unwrap();
    let with_q4 = projector_fetched_with(Quantization::Q4KM, &projectors).unwrap();

    assert_eq!(
        (with_q8.path.as_str(), with_q8.size),
        ("X.mmproj-Q8_0.gguf", 3)
    );
    assert_eq!(
        (with_q4.path.as_str(), with_q4.size),
        ("mmproj-F16.gguf", 2)
    );
    assert!(projector_fetched_with(Quantization::Q8_0, &[]).is_none());
}
