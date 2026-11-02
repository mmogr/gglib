//! Choosing the variant a download fetches.
//!
//! Centralizes the rules for choosing a repository's variant, so each
//! adapter (GUI, CLI, daemon) asks the same way.
//!
//! # Selection Rules
//!
//! 1. A request names a variant by its label ([`match_variant`]): exactly,
//!    ignoring case, else as the one label that holds it as a whole token.
//!    A request that names none, or several, is refused with the labels.
//! 2. With no request, the largest quantized variant that fits the memory
//!    budget ([`pick_variant`]): else the largest that fits, else, or with
//!    no budget, the smallest. A repository of drafts and helpers alone is
//!    refused.
//! 3. A variant whose header says it is a draft, a helper or a projector is
//!    never chosen: asked for, it is refused; with none asked, it is passed
//!    over.
//!
//! # Example
//!
//! ```ignore
//! let selector = QuantizationSelector::new(resolver, variants);
//! let selected = selector.select("user/model", Some("Q8_0"), None).await?;
//! // Or for auto-selection against a budget:
//! let selected = selector.select("user/model", None, Some(budget)).await?;
//! ```

use std::sync::Arc;

use gglib_core::download::{
    Contender, DownloadError, FitJudge, MemoryFit, NoWeights, Pick, PickReason, pick_variant,
};
use gglib_core::ports::huggingface::{label_refusal, match_variant};
use gglib_core::ports::{HfQuantInfo, QuantizationResolver};

use crate::variant_reader::VariantSource;

/// Variant selection service.
///
/// Uses a resolver to list the repository's variants and read their roles,
/// and a source of readers for what the pick reads of them.
pub(crate) struct QuantizationSelector {
    resolver: Arc<dyn QuantizationResolver>,
    variants: Arc<dyn VariantSource>,
}

impl QuantizationSelector {
    /// Create a new selector with the given resolver and variant source.
    pub(crate) fn new(
        resolver: Arc<dyn QuantizationResolver>,
        variants: Arc<dyn VariantSource>,
    ) -> Self {
        Self { resolver, variants }
    }

    /// The label of the variant of `repo_id` a download fetches: the one
    /// `requested` names, or with none the one [`pick_variant`] takes
    /// against `budget_bytes`.
    ///
    /// # Errors
    ///
    /// - the repository has no GGUF files
    /// - the request names no single variant, or names one that is not a
    ///   model
    /// - with no request, the repository holds no model
    pub(crate) async fn select(
        &self,
        repo_id: &str,
        requested: Option<&str>,
        budget_bytes: Option<u64>,
    ) -> Result<String, DownloadError> {
        let available = self.resolver.list_available(repo_id).await?;
        let labels: Vec<&str> = available.iter().map(|v| v.label.as_str()).collect();

        if available.is_empty() {
            return Err(DownloadError::resolution_failed(format!(
                "No GGUF files available for model '{repo_id}'"
            )));
        }

        if let Some(request) = requested {
            let label = match_variant(&available, request)
                .map_err(|mismatch| {
                    DownloadError::other(label_refusal(repo_id, request, &labels, &mismatch))
                })?
                .label
                .clone();
            self.refuse_unless_a_model(repo_id, &available, &label)
                .await?;
            return Ok(label);
        }

        let contenders: Vec<Contender> = available
            .iter()
            .map(|variant| Contender {
                size: variant.total_size,
                role: variant.role,
            })
            .collect();
        let reader = self.variants.reader(repo_id, &available);
        let judge = budget_bytes.map(|budget_bytes| MemoryFit { budget_bytes });
        let pick = pick_variant(
            &contenders,
            reader.as_ref(),
            judge.as_ref().map(|judge| judge as &dyn FitJudge),
        )
        .await
        .map_err(|NoWeights| {
            DownloadError::other(format!("No model in '{repo_id}', {NoWeights}"))
        })?;
        let label = available[pick.index].label.clone();
        log_pick(repo_id, &label, pick, budget_bytes);
        Ok(label)
    }

    /// Refuse the variant labelled `label` among `available` when its
    /// header says it is a draft, a helper or a projector.
    async fn refuse_unless_a_model(
        &self,
        repo_id: &str,
        available: &[HfQuantInfo],
        label: &str,
    ) -> Result<(), DownloadError> {
        let Some(variant) = available.iter().find(|v| v.label == label) else {
            return Ok(());
        };
        let judged = HfQuantInfo {
            role: Some(self.resolver.role_of(repo_id, variant).await),
            ..variant.clone()
        };
        judged
            .refusal()
            .map_or(Ok(()), |why| Err(DownloadError::other(why)))
    }
}

/// Say which variant was taken with none asked, and why.
fn log_pick(repo_id: &str, label: &str, pick: Pick, budget_bytes: Option<u64>) {
    match pick.reason {
        PickReason::Quantized(file_type) => tracing::info!(
            repo_id, selected = label, %file_type, budget_bytes,
            "Auto-selected the largest quantized variant that fits"
        ),
        PickReason::LargestFitting(file_type) => tracing::info!(
            repo_id, selected = label, file_type = ?file_type, budget_bytes,
            "No quantized variant fits; auto-selected the largest that does"
        ),
        PickReason::NoneFits => tracing::warn!(
            repo_id,
            selected = label,
            budget_bytes,
            "No variant fits this machine's memory; auto-selected the smallest"
        ),
        PickReason::NoBudget => tracing::warn!(
            repo_id,
            selected = label,
            "This machine's memory could not be read; auto-selected the smallest variant"
        ),
    }
}

#[cfg(test)]
#[path = "quant_selector_tests.rs"]
mod tests;
