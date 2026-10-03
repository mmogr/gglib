//! Tests for the resolver: the group a download is made of.

use std::sync::Arc;

use gglib_core::download::GgufFileRole;

use super::*;
use crate::quant_selector::QuantizationSelector;
use crate::test_hub::RepoHub;

const REPO: &str = "owner/zeta-GGUF";

fn resolver(files: &[(&str, u64)]) -> HfQuantizationResolver {
    HfQuantizationResolver::new(Arc::new(RepoHub::new(files)))
}

async fn resolve(files: &[(&str, u64)], quantization: Quantization) -> Resolution {
    resolver(files).resolve(REPO, quantization).await.unwrap()
}

fn paths(resolution: &Resolution) -> Vec<&str> {
    resolution.files.iter().map(|f| f.path.as_str()).collect()
}

const THREE_SHARDS: [(&str, u64); 3] = [
    ("zeta.Q8_0-00001-of-00003.gguf", 1_000),
    ("zeta.Q8_0-00002-of-00003.gguf", 1_000),
    ("zeta.Q8_0-00003-of-00003.gguf", 500),
];

// ── The group ────────────────────────────────────────────────────────────

/// The projector's name sorts before the weights'. The weights are still the
/// first file, which is the one a download's primary path is taken from.
#[tokio::test]
async fn the_weights_come_first_whatever_the_names() {
    let resolution = resolve(
        &[("mmproj-F16.gguf", 300), ("zeta.Q8_0.gguf", 1_000)],
        Quantization::Q8_0,
    )
    .await;

    assert_eq!(paths(&resolution), ["zeta.Q8_0.gguf", "mmproj-F16.gguf"]);
    assert_eq!(resolution.files[0].role, GgufFileRole::Weights);
    assert_eq!(resolution.files[1].role, GgufFileRole::Projector);
    assert_eq!(
        resolution.projector().map(|f| f.path.as_str()),
        Some("mmproj-F16.gguf")
    );
}

#[tokio::test]
async fn one_weights_file_and_a_projector_are_one_unsharded_model() {
    let resolution = resolve(
        &[("mmproj-F16.gguf", 300), ("zeta.Q8_0.gguf", 1_000)],
        Quantization::Q8_0,
    )
    .await;

    assert!(!resolution.is_sharded);
    assert_eq!(resolution.shard_count(), 1);
    assert_eq!(resolution.file_count(), 2);
    assert_eq!(resolution.total_size(), Some(1_300));
}

#[tokio::test]
async fn three_shards_and_a_projector_are_three_shards() {
    let mut files = THREE_SHARDS.to_vec();
    files.push(("mmproj-F16.gguf", 300));

    let resolution = resolve(&files, Quantization::Q8_0).await;

    assert!(resolution.is_sharded);
    assert_eq!(resolution.shard_count(), 3);
    assert_eq!(resolution.file_count(), 4);
    assert_eq!(
        paths(&resolution),
        [
            "zeta.Q8_0-00001-of-00003.gguf",
            "zeta.Q8_0-00002-of-00003.gguf",
            "zeta.Q8_0-00003-of-00003.gguf",
            "mmproj-F16.gguf"
        ]
    );
    assert_eq!(resolution.total_size(), Some(2_800));
}

#[tokio::test]
async fn a_repository_without_projectors_resolves_to_its_weights() {
    let one = resolve(&[("zeta.Q8_0.gguf", 1_000)], Quantization::Q8_0).await;
    let three = resolve(&THREE_SHARDS, Quantization::Q8_0).await;

    assert_eq!(paths(&one), ["zeta.Q8_0.gguf"]);
    assert_eq!(one.projector(), None);
    assert!(!one.is_sharded);
    assert_eq!(three.projector(), None);
    assert!(three.is_sharded);
    assert_eq!(three.shard_count(), 3);
}

#[tokio::test]
async fn each_file_keeps_its_size_and_oid() {
    let resolution = resolve(
        &[("mmproj-F16.gguf", 300), ("zeta.Q8_0.gguf", 1_000)],
        Quantization::Q8_0,
    )
    .await;

    let weights = &resolution.files[0];
    let projector = &resolution.files[1];
    assert_eq!(weights.size, Some(1_000));
    assert_eq!(weights.oid.as_deref(), Some("oid-zeta.Q8_0.gguf"));
    assert_eq!(projector.size, Some(300));
    assert_eq!(projector.oid.as_deref(), Some("oid-mmproj-F16.gguf"));
}

/// The pre-split shape: the projector of the same quantization was the
/// group's second "shard". It is the group's projector now.
#[tokio::test]
async fn a_projector_of_the_same_quantization_is_the_projector_not_a_shard() {
    let resolution = resolve(
        &[("X.Q8_0.gguf", 8_000), ("X.mmproj-Q8_0.gguf", 600)],
        Quantization::Q8_0,
    )
    .await;

    assert_eq!(paths(&resolution), ["X.Q8_0.gguf", "X.mmproj-Q8_0.gguf"]);
    assert_eq!(resolution.shard_count(), 1);
    assert!(!resolution.is_sharded);
    assert_eq!(resolution.files[1].role, GgufFileRole::Projector);
}

// ── Which projector ──────────────────────────────────────────────────────

#[tokio::test]
async fn the_projector_is_chosen_by_the_one_rule() {
    let weights = ("zeta.Q8_0.gguf", 1_000);
    let chosen = |projectors: &[(&'static str, u64)]| {
        let mut files = vec![weights];
        files.extend_from_slice(projectors);
        async move {
            let resolution = resolve(&files, Quantization::Q8_0).await;
            resolution.projector().map(|f| f.path.clone())
        }
    };

    let same_quant = chosen(&[("mmproj-F16.gguf", 3), ("zeta.mmproj-Q8_0.gguf", 2)]).await;
    let f16 = chosen(&[("mmproj-BF16.gguf", 3), ("mmproj-F16.gguf", 2)]).await;
    let first = chosen(&[("mmproj-Q4_0.gguf", 3), ("mmproj-BF16.gguf", 2)]).await;

    assert_eq!(same_quant.as_deref(), Some("zeta.mmproj-Q8_0.gguf"));
    assert_eq!(f16.as_deref(), Some("mmproj-F16.gguf"));
    assert_eq!(first.as_deref(), Some("mmproj-BF16.gguf"));
}

// ── What is not a download ───────────────────────────────────────────────

#[tokio::test]
async fn a_repository_of_only_projectors_resolves_to_nothing() {
    let only_projectors = resolver(&[("mmproj-F16.gguf", 300), ("mmproj-Q8_0.gguf", 200)]);

    let refused = only_projectors.resolve(REPO, Quantization::F16).await;
    let available = only_projectors.list_available(REPO).await.unwrap();

    let message = refused.unwrap_err().to_string();
    assert!(message.contains("F16"), "{message}");
    assert!(available.is_empty());
}

#[tokio::test]
async fn a_quantization_the_repository_lacks_fails_to_resolve() {
    let refused = resolver(&[("zeta.Q8_0.gguf", 1_000)])
        .resolve(REPO, Quantization::Q4KM)
        .await;

    assert!(matches!(
        refused,
        Err(DownloadError::ResolutionFailed { .. })
    ));
}

// ── The quantizations on offer ───────────────────────────────────────────

#[tokio::test]
async fn the_available_quantizations_are_those_of_the_weights() {
    let available = resolver(&[
        ("zeta.Q8_0.gguf", 8_000),
        ("zeta.Q4_K_M.gguf", 4_000),
        ("mmproj-F16.gguf", 300),
        ("mmproj-BF16.gguf", 300),
    ])
    .list_available(REPO)
    .await
    .unwrap();

    assert_eq!(available, [Quantization::Q4KM, Quantization::Q8_0]);
}

/// One quantization and two projectors is still "one option": the automatic
/// pick takes it, and does not ask the user to choose between the weights
/// and a projector's `F16`.
#[tokio::test]
async fn the_single_quantization_pick_ignores_projectors() {
    let hub = resolver(&[
        ("zeta.IQ2_XXS.gguf", 2_000),
        ("mmproj-F16.gguf", 300),
        ("mmproj-BF16.gguf", 300),
    ]);
    let selector = QuantizationSelector::new(Arc::new(hub));

    let selection = selector.select(REPO, None).await.unwrap();

    assert_eq!(selection.quantization, Quantization::Iq2Xxs);
    assert!(selection.auto_selected);
    assert_eq!(selection.available, [Quantization::Iq2Xxs]);
}
