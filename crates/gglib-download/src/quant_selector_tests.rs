//! Tests for choosing a variant: by its label, and with none asked.

use super::*;
use async_trait::async_trait;
use gglib_core::download::GgufFileRole;
use gglib_core::ports::Resolution;

/// A resolver listing `variants`, whose roles its headers would give.
struct MockResolver {
    available: Vec<HfQuantInfo>,
}

fn variant(label: &str, role: GgufFileRole) -> HfQuantInfo {
    HfQuantInfo {
        label: label.to_owned(),
        role: Some(role),
        file_paths: vec![format!("model-{label}.gguf")],
        total_size: 1,
    }
}

impl MockResolver {
    fn new(labels: &[&str]) -> Self {
        Self::with(
            &labels
                .iter()
                .map(|label| (*label, GgufFileRole::Weights))
                .collect::<Vec<_>>(),
        )
    }

    fn with(variants: &[(&str, GgufFileRole)]) -> Self {
        Self {
            available: variants
                .iter()
                .map(|(label, role)| variant(label, *role))
                .collect(),
        }
    }
}

#[async_trait]
impl QuantizationResolver for MockResolver {
    async fn resolve(&self, _repo_id: &str, _label: &str) -> Result<Resolution, DownloadError> {
        unimplemented!("not needed for selection tests")
    }

    async fn list_available(&self, _repo_id: &str) -> Result<Vec<HfQuantInfo>, DownloadError> {
        Ok(self.available.clone())
    }

    async fn role_of(&self, _repo_id: &str, variant: &HfQuantInfo) -> GgufFileRole {
        variant.role.unwrap_or_default()
    }
}

async fn select(resolver: MockResolver, request: Option<&str>) -> Result<String, String> {
    QuantizationSelector::new(Arc::new(resolver))
        .select("test/model", request)
        .await
        .map(|selection| selection.label)
        .map_err(|refused| refused.to_string())
}

#[tokio::test]
async fn a_request_names_its_label_ignoring_case_or_as_a_token() {
    let labels = ["Q4_K_M", "Q8_0", "UD-Q4_K_XL", "Q3_K_P"];
    assert_eq!(
        select(MockResolver::new(&labels), Some("q8_0")).await,
        Ok("Q8_0".into())
    );
    assert_eq!(
        select(MockResolver::new(&labels), Some("Q4_K_XL")).await,
        Ok("UD-Q4_K_XL".into())
    );
    assert_eq!(
        select(MockResolver::new(&labels), Some("Q3_K_P")).await,
        Ok("Q3_K_P".into())
    );
}

#[tokio::test]
async fn a_request_that_names_no_single_label_is_refused_with_the_labels() {
    let refused = select(MockResolver::new(&["Q8_0", "Q4_K_M"]), Some("Q5_K_M"))
        .await
        .unwrap_err();
    assert!(refused.contains("'Q5_K_M'"), "{refused}");
    assert!(refused.contains("Q8_0, Q4_K_M"), "{refused}");

    let refused = select(MockResolver::new(&["Q4_K_M", "Q4_K_S"]), Some("Q4"))
        .await
        .unwrap_err();
    assert!(refused.contains("Q4_K_M, Q4_K_S"), "{refused}");
}

#[tokio::test]
async fn a_draft_asked_for_is_refused() {
    let resolver = MockResolver::with(&[
        ("Q4_K_P", GgufFileRole::Weights),
        ("FastMTP-32K", GgufFileRole::Sidecar),
    ]);
    let refused = select(resolver, Some("FastMTP-32K")).await.unwrap_err();
    assert!(
        refused.contains("FastMTP-32K is a draft or helper file"),
        "{refused}"
    );
    assert!(
        refused.contains("gglib can't run draft or helper files"),
        "{refused}"
    );
}

#[tokio::test]
async fn the_one_model_is_taken_whatever_lies_beside_it() {
    assert_eq!(
        select(MockResolver::new(&["IQ1_M"]), None).await,
        Ok("IQ1_M".into())
    );
    let beside = MockResolver::with(&[
        ("IQ2_XXS", GgufFileRole::Weights),
        ("mmproj-F16", GgufFileRole::Projector),
        ("FastMTP", GgufFileRole::Sidecar),
    ]);
    assert_eq!(select(beside, None).await, Ok("IQ2_XXS".into()));
    let only_a_draft = MockResolver::with(&[("mtp-Q8_0", GgufFileRole::Sidecar)]);
    let refused = select(only_a_draft, None).await.unwrap_err();
    assert!(refused.contains("only draft or helper files"), "{refused}");
}

#[tokio::test]
async fn the_preference_list_matches_labels_and_passes_over_drafts() {
    assert_eq!(
        select(MockResolver::new(&["Q8_0", "Q4_K_M", "F16"]), None).await,
        Ok("Q4_K_M".into()),
        "Q4_K_M comes before Q8_0 on the list"
    );
    let resolver = MockResolver::with(&[
        ("Q4_K_M-mtp", GgufFileRole::Sidecar),
        ("Q5_K_M-mtp", GgufFileRole::Sidecar),
        ("Q8_0", GgufFileRole::Weights),
        ("Q8_0-mtp", GgufFileRole::Sidecar),
    ]);
    assert_eq!(select(resolver, None).await, Ok("Q8_0".into()));
    let resolver = MockResolver::new(&["UD-Q4_K_XL", "UD-Q6_K"]);
    assert_eq!(
        select(resolver, None).await,
        Ok("UD-Q6_K".into()),
        "Q6_K names UD-Q6_K as a token"
    );
}

#[tokio::test]
async fn no_variants_is_an_error() {
    let refused = select(MockResolver::new(&[]), None).await.unwrap_err();
    assert!(refused.contains("No GGUF files"), "{refused}");
}

#[tokio::test]
async fn no_preference_naming_a_model_requires_a_choice() {
    let refused = select(MockResolver::new(&["IQ2_XXS", "IQ1_M"]), None)
        .await
        .unwrap_err();
    assert!(
        refused.contains("please select one: IQ2_XXS, IQ1_M"),
        "{refused}"
    );
}
