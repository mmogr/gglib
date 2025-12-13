//! Quantization selection logic.
//!
//! Centralizes the rules for selecting a quantization when the user hasn't
//! specified one explicitly. This prevents each adapter (GUI, CLI, Tauri)
//! from implementing their own (potentially inconsistent) selection logic.
//!
//! # Selection Rules
//!
//! 1. If a quantization is provided, validate it exists in the repository
//! 2. If none is provided:
//!    - 0 options available → error (empty repository)
//!    - 1 option available → auto-pick it (pre-quantized model)
//!    - >1 options available → use default preference list, else error
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

use gglib_core::download::{DownloadError, Quantization};
use gglib_core::ports::QuantizationResolver;

/// Default quantization preference order.
///
/// When no quantization is specified and multiple are available,
/// we try these in order and pick the first one that exists.
const DEFAULT_QUANT_PREFERENCE: &[&str] = &["Q5_K_M", "Q4_K_M", "Q5_K_S", "Q4_K_S", "Q6_K", "Q8_0"];

/// Result of quantization selection.
#[derive(Debug, Clone)]
pub struct QuantizationSelection {
    /// The selected quantization.
    pub quantization: Quantization,
    /// Whether this was auto-selected (vs. explicitly requested).
    pub auto_selected: bool,
    /// The available quantizations in the repository.
    pub available: Vec<Quantization>,
}

/// Error details for quantization selection failures.
#[derive(Debug, Clone)]
pub enum SelectionError {
    /// No quantizations available in the repository.
    NoQuantizationsAvailable { repo_id: String },
    /// Requested quantization not found.
    QuantizationNotFound {
        repo_id: String,
        requested: String,
        available: Vec<String>,
    },
    /// Multiple quantizations available but none match defaults.
    SelectionRequired {
        repo_id: String,
        available: Vec<String>,
    },
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoQuantizationsAvailable { repo_id } => {
                write!(f, "No quantizations available for model '{repo_id}'")
            }
            Self::QuantizationNotFound {
                repo_id,
                requested,
                available,
            } => {
                write!(
                    f,
                    "Quantization '{requested}' not found for model '{repo_id}'. Available: {}",
                    available.join(", ")
                )
            }
            Self::SelectionRequired { repo_id, available } => {
                write!(
                    f,
                    "Multiple quantizations available for '{repo_id}', please select one: {}",
                    available.join(", ")
                )
            }
        }
    }
}

impl From<SelectionError> for DownloadError {
    fn from(err: SelectionError) -> Self {
        match err {
            SelectionError::NoQuantizationsAvailable { repo_id } => {
                Self::resolution_failed(format!("No quantizations available for model '{repo_id}'"))
            }
            SelectionError::QuantizationNotFound {
                requested,
                available,
                ..
            } => Self::InvalidQuantization {
                value: format!("{} (available: {})", requested, available.join(", ")),
            },
            SelectionError::SelectionRequired { available, .. } => {
                Self::resolution_failed(format!(
                    "Multiple quantizations available, please select one: {}",
                    available.join(", ")
                ))
            }
        }
    }
}

/// Quantization selection service.
///
/// Uses a resolver to list available quantizations and applies selection rules.
pub struct QuantizationSelector {
    resolver: Arc<dyn QuantizationResolver>,
}

impl QuantizationSelector {
    /// Create a new selector with the given resolver.
    pub fn new(resolver: Arc<dyn QuantizationResolver>) -> Self {
        Self { resolver }
    }

    /// Select a quantization for download.
    ///
    /// # Arguments
    ///
    /// * `repo_id` - The `HuggingFace` repository ID
    /// * `requested` - Optional user-requested quantization (e.g., "`Q8_0`")
    ///
    /// # Returns
    ///
    /// Returns the selected quantization and metadata about the selection.
    ///
    /// # Errors
    ///
    /// - `NoQuantizationsAvailable` if the repo has no GGUF files
    /// - `QuantizationNotFound` if the requested quant doesn't exist
    /// - `SelectionRequired` if multiple quants exist but none match defaults
    pub async fn select(
        &self,
        repo_id: &str,
        requested: Option<&str>,
    ) -> Result<QuantizationSelection, DownloadError> {
        // Get available quantizations
        let available = self.resolver.list_available(repo_id).await?;
        let available_strings: Vec<String> = available.iter().map(ToString::to_string).collect();

        // Case: No quantizations available
        if available.is_empty() {
            return Err(SelectionError::NoQuantizationsAvailable {
                repo_id: repo_id.to_string(),
            }
            .into());
        }

        // Case: User specified a quantization - validate it exists
        if let Some(req) = requested {
            // Parse the requested quantization
            let synthetic_filename = format!("model-{req}.gguf");
            let quant = Quantization::from_filename(&synthetic_filename);

            // Check if it exists in available list
            if available.contains(&quant) {
                return Ok(QuantizationSelection {
                    quantization: quant,
                    auto_selected: false,
                    available,
                });
            }

            // Also try exact string match (handles edge cases)
            for avail in &available {
                if avail.to_string().eq_ignore_ascii_case(req) {
                    return Ok(QuantizationSelection {
                        quantization: *avail,
                        auto_selected: false,
                        available,
                    });
                }
            }

            // Not found
            return Err(SelectionError::QuantizationNotFound {
                repo_id: repo_id.to_string(),
                requested: req.to_string(),
                available: available_strings,
            }
            .into());
        }

        // Case: Single quantization available - auto-pick it
        if available.len() == 1 {
            return Ok(QuantizationSelection {
                quantization: available[0],
                auto_selected: true,
                available,
            });
        }

        // Case: Multiple quantizations - try default preference list
        for pref in DEFAULT_QUANT_PREFERENCE {
            let synthetic_filename = format!("model-{pref}.gguf");
            let quant = Quantization::from_filename(&synthetic_filename);

            if available.contains(&quant) {
                tracing::info!(
                    repo_id = %repo_id,
                    selected = %quant,
                    "Auto-selected quantization from default preference list"
                );
                return Ok(QuantizationSelection {
                    quantization: quant,
                    auto_selected: true,
                    available,
                });
            }
        }

        // No default preference matched - require explicit selection
        Err(SelectionError::SelectionRequired {
            repo_id: repo_id.to_string(),
            available: available_strings,
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use gglib_core::ports::Resolution;

    /// Mock resolver for testing.
    struct MockResolver {
        available: Vec<Quantization>,
    }

    impl MockResolver {
        fn new(quants: &[&str]) -> Self {
            let available = quants
                .iter()
                .map(|q| {
                    let filename = format!("model-{q}.gguf");
                    Quantization::from_filename(&filename)
                })
                .collect();
            Self { available }
        }

        fn empty() -> Self {
            Self { available: vec![] }
        }
    }

    #[async_trait]
    impl QuantizationResolver for MockResolver {
        async fn resolve(
            &self,
            _repo_id: &str,
            _quantization: Quantization,
        ) -> Result<Resolution, DownloadError> {
            unimplemented!("not needed for selection tests")
        }

        async fn list_available(&self, _repo_id: &str) -> Result<Vec<Quantization>, DownloadError> {
            Ok(self.available.clone())
        }
    }

    #[tokio::test]
    async fn test_explicit_quant_exists() {
        let resolver = Arc::new(MockResolver::new(&["Q4_K_M", "Q8_0", "F16"]));
        let selector = QuantizationSelector::new(resolver);

        let result = selector.select("test/model", Some("Q8_0")).await.unwrap();
        assert_eq!(result.quantization.to_string(), "Q8_0");
        assert!(!result.auto_selected);
    }

    #[tokio::test]
    async fn test_explicit_quant_not_found() {
        let resolver = Arc::new(MockResolver::new(&["Q8_0"]));
        let selector = QuantizationSelector::new(resolver);

        let result = selector.select("test/model", Some("Q4_K_M")).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Q4_K_M"));
    }

    #[tokio::test]
    async fn test_single_quant_auto_selected() {
        let resolver = Arc::new(MockResolver::new(&["Q8_0"]));
        let selector = QuantizationSelector::new(resolver);

        let result = selector.select("test/model", None).await.unwrap();
        assert_eq!(result.quantization.to_string(), "Q8_0");
        assert!(result.auto_selected);
    }

    #[tokio::test]
    async fn test_multiple_quants_default_preference() {
        let resolver = Arc::new(MockResolver::new(&["Q8_0", "Q4_K_M", "F16"]));
        let selector = QuantizationSelector::new(resolver);

        let result = selector.select("test/model", None).await.unwrap();
        // Q4_K_M is in the default preference list before Q8_0
        assert_eq!(result.quantization.to_string(), "Q4_K_M");
        assert!(result.auto_selected);
    }

    #[tokio::test]
    async fn test_no_quants_available() {
        let resolver = Arc::new(MockResolver::empty());
        let selector = QuantizationSelector::new(resolver);

        let result = selector.select("test/model", None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No quantizations"));
    }

    #[tokio::test]
    async fn test_no_default_match_requires_selection() {
        // Use quantizations that aren't in the default preference list
        let resolver = Arc::new(MockResolver::new(&["IQ2_XXS", "IQ1_M"]));
        let selector = QuantizationSelector::new(resolver);

        let result = selector.select("test/model", None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("please select one"));
    }
}
