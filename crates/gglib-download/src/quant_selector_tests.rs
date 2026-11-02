//! Tests for [`super::QuantizationSelector`].

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::FileType;
use gglib_core::download::{DownloadError, GgufFileRole, HeadFacts, VariantReader};
use gglib_core::ports::{
    GgufMetadata, HeadMetadata, HfQuantInfo, QuantizationResolver, Resolution,
};

use super::QuantizationSelector;
use crate::variant_reader::VariantSource;

const GB: u64 = 1_000_000_000;

/// A repository's variants: each a label, a size, the role its header gives,
/// and the type it states.
#[derive(Clone)]
struct Repo(Vec<(&'static str, u64, GgufFileRole, FileType)>);

impl Repo {
    /// Weights variants of `labels`, each `Q8_0` and a byte in size.
    fn of(labels: &[&'static str]) -> Self {
        Self(
            labels
                .iter()
                .map(|label| (*label, 1, GgufFileRole::Weights, FileType(7)))
                .collect(),
        )
    }
}

#[async_trait]
impl QuantizationResolver for Repo {
    async fn resolve(&self, _repo_id: &str, _label: &str) -> Result<Resolution, DownloadError> {
        unimplemented!("not needed for selection tests")
    }

    /// The listing gives no role: the pick and a request read it.
    async fn list_available(&self, _repo_id: &str) -> Result<Vec<HfQuantInfo>, DownloadError> {
        Ok(self
            .0
            .iter()
            .map(|(label, size, _, _)| HfQuantInfo {
                label: (*label).to_owned(),
                role: None,
                file_paths: vec![format!("model-{label}.gguf")],
                total_size: *size,
            })
            .collect())
    }

    async fn role_of(&self, _repo_id: &str, variant: &HfQuantInfo) -> GgufFileRole {
        self.0
            .iter()
            .find(|(label, ..)| *label == variant.label)
            .map(|(_, _, role, _)| *role)
            .unwrap_or_default()
    }
}

/// Each variant's whole header in its head, read as an image model's, so
/// its fit is its size against the budget's usable share.
impl VariantSource for Repo {
    fn reader<'a>(
        &'a self,
        _repo_id: &'a str,
        _variants: &[HfQuantInfo],
    ) -> Box<dyn VariantReader + 'a> {
        Box::new(self.clone())
    }
}

#[async_trait]
impl VariantReader for Repo {
    async fn head(&self, index: usize) -> Option<HeadFacts> {
        let metadata = self.header(index).await?;
        Some(HeadFacts {
            head: HeadMetadata {
                role: Some(metadata.role),
                metadata,
                complete: true,
                ..HeadMetadata::default()
            },
            image: true,
        })
    }

    async fn header(&self, index: usize) -> Option<GgufMetadata> {
        let (_, _, role, file_type) = *self.0.get(index)?;
        Some(GgufMetadata {
            role,
            file_type: Some(file_type),
            ..GgufMetadata::default()
        })
    }
}

async fn select(repo: Repo, request: Option<&str>, budget: Option<u64>) -> Result<String, String> {
    let repo = Arc::new(repo);
    QuantizationSelector::new(repo.clone(), repo)
        .select("test/model", request, budget)
        .await
        .map_err(|refused| refused.to_string())
}

#[tokio::test]
async fn a_request_names_its_label_ignoring_case_or_as_a_token() {
    let labels = ["Q4_K_M", "Q8_0", "UD-Q4_K_XL", "Q3_K_P"];
    assert_eq!(
        select(Repo::of(&labels), Some("q8_0"), None).await,
        Ok("Q8_0".into())
    );
    assert_eq!(
        select(Repo::of(&labels), Some("Q4_K_XL"), None).await,
        Ok("UD-Q4_K_XL".into())
    );
    assert_eq!(
        select(Repo::of(&labels), Some("Q3_K_P"), None).await,
        Ok("Q3_K_P".into())
    );
}

#[tokio::test]
async fn a_request_that_names_no_single_label_is_refused_with_the_labels() {
    let refused = select(Repo::of(&["Q8_0", "Q4_K_M"]), Some("Q5_K_M"), None)
        .await
        .unwrap_err();
    assert!(refused.contains("'Q5_K_M'"), "{refused}");
    assert!(refused.contains("Q8_0, Q4_K_M"), "{refused}");

    let refused = select(Repo::of(&["Q4_K_M", "Q4_K_S"]), Some("Q4"), None)
        .await
        .unwrap_err();
    assert!(refused.contains("Q4_K_M, Q4_K_S"), "{refused}");
}

#[tokio::test]
async fn a_draft_asked_for_is_refused() {
    let repo = Repo(vec![
        ("Q4_K_P", 1, GgufFileRole::Weights, FileType(15)),
        ("FastMTP-32K", 1, GgufFileRole::Sidecar, FileType(7)),
    ]);
    let refused = select(repo, Some("FastMTP-32K"), None).await.unwrap_err();
    assert!(
        refused.contains("FastMTP-32K is a draft or helper file"),
        "{refused}"
    );
    assert!(
        refused.contains("gglib can't run draft or helper files"),
        "{refused}"
    );
}

/// The real `HauhauCS` layout, sizes rounded: the largest quantized variant
/// that fits a 32 GB budget's usable share is `Q8_0`; the bigger `BF16`
/// does not fit, and the MTP draft and projector are passed over.
#[tokio::test]
async fn with_none_asked_the_largest_quantized_variant_that_fits_is_taken() {
    let repo = Repo(vec![
        ("mmproj-F16", 1, GgufFileRole::Projector, FileType(1)),
        ("FastMTP-32K", 2 * GB, GgufFileRole::Sidecar, FileType(7)),
        ("Q4_K_P", 17 * GB, GgufFileRole::Weights, FileType(15)),
        ("Q6_K_P", 22 * GB, GgufFileRole::Weights, FileType(18)),
        ("Q8_0", 28 * GB, GgufFileRole::Weights, FileType(7)),
        ("BF16", 54 * GB, GgufFileRole::Weights, FileType(32)),
    ]);
    assert_eq!(
        select(repo.clone(), None, Some(32 * GB)).await,
        Ok("Q8_0".into())
    );
    assert_eq!(
        select(repo.clone(), None, Some(26 * GB)).await,
        Ok("Q6_K_P".into())
    );
    assert_eq!(
        select(repo.clone(), None, Some(8 * GB)).await,
        Ok("Q4_K_P".into()),
        "none fits: the smallest model"
    );
    assert_eq!(
        select(repo, None, None).await,
        Ok("Q4_K_P".into()),
        "no budget: the smallest model"
    );
}

#[tokio::test]
async fn with_none_asked_a_repository_of_drafts_alone_is_refused() {
    let repo = Repo(vec![("mtp-Q8_0", 1, GgufFileRole::Sidecar, FileType(7))]);
    let refused = select(repo, None, Some(32 * GB)).await.unwrap_err();
    assert_eq!(
        refused,
        "No model in 'test/model', only draft or helper files: \
         gglib can't run draft or helper files on their own"
    );
}

#[tokio::test]
async fn no_variants_is_an_error() {
    let refused = select(Repo::of(&[]), None, None).await.unwrap_err();
    assert!(refused.contains("No GGUF files"), "{refused}");
}
