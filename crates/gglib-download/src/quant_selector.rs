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
//! 2. With no request:
//!    - no variant → error (empty repository)
//!    - the first entry of the default preference list that names a model
//!      the same way
//!    - else the one model there is, beside drafts, helpers and projectors
//!    - else an error listing the models, or saying there are none
//! 3. A variant whose header says it is a draft, a helper or a projector is
//!    never chosen: asked for, it is refused; on the preference list, it is
//!    passed over.
//!
//! # Example
//!
//! ```ignore
//! let selector = QuantizationSelector::new(resolver);
//! let selected = selector.select("user/model", Some("Q8_0")).await?;
//! // Or for auto-selection:
//! let selected = selector.select("user/model", None).await?;
//! ```

use std::sync::Arc;

use gglib_core::download::DownloadError;
use gglib_core::ports::huggingface::{HELPER_FILES_REFUSAL, label_refusal, match_variant};
use gglib_core::ports::{HfQuantInfo, QuantizationResolver};

/// Default quantization preference order.
///
/// When no variant is asked for and several are available, each entry is
/// matched against the labels as a request is, in order, and the first
/// model it names is taken.
const DEFAULT_QUANT_PREFERENCE: &[&str] = &["Q5_K_M", "Q4_K_M", "Q5_K_S", "Q4_K_S", "Q6_K", "Q8_0"];

/// Result of choosing a variant.
#[derive(Debug, Clone)]
pub(crate) struct QuantizationSelection {
    /// The label of the variant chosen.
    pub label: String,
    /// Whether this was auto-selected (vs. explicitly requested).
    pub auto_selected: bool,
    /// The repository's variants, as its listing gave them.
    pub available: Vec<HfQuantInfo>,
}

/// Variant selection service.
///
/// Uses a resolver to list the repository's variants and read their roles.
pub(crate) struct QuantizationSelector {
    resolver: Arc<dyn QuantizationResolver>,
}

impl QuantizationSelector {
    /// Create a new selector with the given resolver.
    pub(crate) fn new(resolver: Arc<dyn QuantizationResolver>) -> Self {
        Self { resolver }
    }

    /// Choose the variant of `repo_id` a download fetches: the one
    /// `requested` names, or one chosen by the rules above.
    ///
    /// # Errors
    ///
    /// - the repository has no GGUF files
    /// - the request names no single variant, or names one that is not a
    ///   model
    /// - with no request, no preference names a model and there is not
    ///   exactly one model
    pub(crate) async fn select(
        &self,
        repo_id: &str,
        requested: Option<&str>,
    ) -> Result<QuantizationSelection, DownloadError> {
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
            return Ok(QuantizationSelection {
                label,
                auto_selected: false,
                available,
            });
        }

        for preference in DEFAULT_QUANT_PREFERENCE {
            let Ok(variant) = match_variant(&available, preference) else {
                continue;
            };
            let label = variant.label.as_str();
            if !self.resolver.role_of(repo_id, variant).await.is_weights() {
                continue;
            }
            tracing::info!(
                repo_id = %repo_id,
                selected = %label,
                "Auto-selected a variant from the default preference list"
            );
            return Ok(QuantizationSelection {
                label: label.to_owned(),
                auto_selected: true,
                available,
            });
        }

        // No preference names a model: the one model there is, when there
        // is one, beside any drafts, helpers and projectors.
        let mut models = Vec::new();
        for variant in &available {
            if self.resolver.role_of(repo_id, variant).await.is_weights() {
                models.push(variant.label.as_str());
            }
        }
        match models.as_slice() {
            [only] => {
                let label = (*only).to_owned();
                Ok(QuantizationSelection {
                    label,
                    auto_selected: true,
                    available,
                })
            }
            [] => Err(DownloadError::other(format!(
                "No model in '{repo_id}', only draft or helper files: {HELPER_FILES_REFUSAL}"
            ))),
            several => Err(DownloadError::resolution_failed(format!(
                "Multiple variants available for '{repo_id}', please select one: {}",
                several.join(", ")
            ))),
        }
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

#[cfg(test)]
#[path = "quant_selector_tests.rs"]
mod tests;
