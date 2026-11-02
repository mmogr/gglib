//! Tests that a repository's projectors come out of its listing as variants
//! of their own, never a shard of the weights.

use super::HfClient;
use crate::client::tests::test_config;
use crate::error::HfError;
use crate::http::testing::{CannedResponse, FakeBackend};
use crate::models::HfRepoRef;
use serde_json::json;

/// A repository whose root holds `Q8_0` weights, the projector of that
/// quantization, and an `F16` projector.
fn client() -> HfClient<FakeBackend> {
    let backend = FakeBackend::new().with_response(
        "tree/main",
        CannedResponse::of(json!([
            {"path": "X.Q8_0.gguf", "type": "file", "size": 8_000, "lfs": {"oid": "w"}},
            {"path": "X.mmproj-Q8_0.gguf", "type": "file", "size": 600, "lfs": {"oid": "p8"}},
            {"path": "mmproj-F16.gguf", "type": "file", "size": 900, "lfs": {"oid": "pf"}},
        ])),
    );
    HfClient::with_backend(test_config(), backend)
}

fn repo() -> HfRepoRef {
    HfRepoRef::new("owner", "X-GGUF")
}

#[tokio::test]
async fn each_projector_is_a_variant_apart_and_never_a_shard() {
    let variants = client().list_variants(&repo()).await.unwrap();

    let shapes: Vec<(&str, &[String])> = variants
        .iter()
        .map(|v| (v.label.as_str(), v.file_paths.as_slice()))
        .collect();
    assert_eq!(
        shapes,
        [
            ("X.Q8_0", &["X.Q8_0.gguf".to_owned()][..]),
            ("X.mmproj-Q8_0", &["X.mmproj-Q8_0.gguf".to_owned()][..]),
            ("mmproj-F16", &["mmproj-F16.gguf".to_owned()][..]),
        ]
    );
}

#[tokio::test]
async fn the_files_of_a_variant_are_its_own_alone() {
    let files = client()
        .find_variant_files(&repo(), "X.Q8_0")
        .await
        .unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "X.Q8_0.gguf");
}

/// `Q4` is in no label here.
#[tokio::test]
async fn a_label_no_variant_holds_is_not_found() {
    let refused = client().find_variant_files(&repo(), "Q4").await;

    assert!(matches!(refused, Err(HfError::QuantizationNotFound { .. })));
}
