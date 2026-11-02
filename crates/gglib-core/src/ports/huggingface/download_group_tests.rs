//! Tests for [`download_group`] and [`largest_projector`].

use super::*;
use crate::ports::huggingface::HfPortError;
use crate::ports::huggingface::fake_hub::{FakeHub, RoleParser, hub_file};

fn paths(group: &DownloadGroup) -> Vec<&str> {
    group.files().map(|f| f.path.as_str()).collect()
}

/// The projector is the largest file whose head says it is one, whatever
/// its name says.
#[tokio::test]
async fn the_group_is_the_weights_then_the_largest_projector() {
    let hub = FakeHub {
        weights: vec![
            hub_file("m-Q8_0-00001-of-00002.gguf", 10, "w1"),
            hub_file("m-Q8_0-00002-of-00002.gguf", 5, "w2"),
        ],
        projectors: vec![
            hub_file("mmproj-F16.gguf", 2, "pf"),
            hub_file("vision-BF16.gguf", 3, "pb"),
        ],
        ..Default::default()
    };

    let group = download_group(&hub, Some(&RoleParser), "o/r", "Q8_0")
        .await
        .unwrap();

    assert_eq!(
        paths(&group),
        [
            "m-Q8_0-00001-of-00002.gguf",
            "m-Q8_0-00002-of-00002.gguf",
            "vision-BF16.gguf"
        ]
    );
    let projector = group.projector.expect("the largest projector");
    assert_eq!(projector.oid.as_deref(), Some("pb"));
}

/// A file named as a projector whose head says weights is not fetched as
/// one.
#[tokio::test]
async fn a_file_named_as_a_projector_is_one_only_if_its_head_says_so() {
    let hub = FakeHub {
        weights: vec![
            hub_file("m-Q8_0.gguf", 10, "w"),
            hub_file("mmproj-lookalike.gguf", 50, "l"),
        ],
        heads: vec![("mmproj-lookalike.gguf".to_owned(), b"weights".to_vec())],
        ..Default::default()
    };

    let group = download_group(&hub, Some(&RoleParser), "o/r", "Q8_0")
        .await
        .unwrap();

    assert!(group.projector.is_none());
}

#[tokio::test]
async fn a_repository_without_projectors_gives_the_weights_alone() {
    let hub = FakeHub {
        weights: vec![hub_file("m-Q8_0.gguf", 10, "w")],
        ..Default::default()
    };

    let group = download_group(&hub, Some(&RoleParser), "o/r", "Q8_0")
        .await
        .unwrap();

    assert_eq!(paths(&group), ["m-Q8_0.gguf"]);
    assert!(group.projector.is_none());
}

/// With no parser no head is read, and no projector fetched.
#[tokio::test]
async fn without_a_parser_no_projector_is_fetched() {
    let hub = FakeHub {
        weights: vec![hub_file("m-Q8_0.gguf", 10, "w")],
        projectors: vec![hub_file("mmproj-F16.gguf", 2, "pf")],
        ..Default::default()
    };

    let group = download_group(&hub, None, "o/r", "Q8_0").await.unwrap();

    assert_eq!(paths(&group), ["m-Q8_0.gguf"]);
    assert!(hub.heads_asked.lock().unwrap().is_empty());
}

/// Projectors alone are not a download: the missing variant is the answer,
/// not a group of one projector.
#[tokio::test]
async fn a_variant_the_repository_lacks_is_not_found() {
    let hub = FakeHub {
        projectors: vec![hub_file("mmproj-F16.gguf", 2, "pf")],
        ..Default::default()
    };

    let refused = download_group(&hub, Some(&RoleParser), "o/r", "Q8_0").await;

    assert!(matches!(
        refused,
        Err(HfPortError::QuantizationNotFound { .. })
    ));
}

fn variant(label: &str, role: GgufFileRole, total_size: u64) -> HfQuantInfo {
    HfQuantInfo {
        label: label.to_owned(),
        role: Some(role),
        file_paths: vec![format!("{label}.gguf")],
        total_size,
    }
}

#[test]
fn the_largest_projector_is_taken_the_first_by_label_of_a_tie() {
    let variants = [
        variant("Q8_0", GgufFileRole::Weights, 900),
        variant("mmproj-F16", GgufFileRole::Projector, 600),
        variant("mmproj-BF16", GgufFileRole::Projector, 600),
        variant("mmproj-Q8_0", GgufFileRole::Projector, 300),
        variant("mtp", GgufFileRole::Sidecar, 800),
    ];

    let largest = largest_projector(&variants).unwrap();

    assert_eq!(largest.label, "mmproj-BF16");
    assert!(largest_projector(&variants[..1]).is_none());
}
