//! The resolved per-model context every request pipeline is built from.

use crate::domain::{InferenceConfig, ModelCapabilities};
use crate::ports::ModelSummary;

/// Everything a request pipeline needs to know about the target model,
/// gathered in a single catalog round-trip.
///
/// The three fields feed three different stages, which is why they travel
/// together rather than being looked up where each is needed:
///
/// * [`capabilities`](Self::capabilities) — request-side transforms
///   (strict-turn coalescing and friends).
/// * [`tags`](Self::tags) — response-stream parser selection.
/// * [`inference_defaults`](Self::inference_defaults) — the per-model layer of
///   the sampling hierarchy.
///
/// Before this type was shared, the proxy resolved all three while every other
/// surface resolved the same row and kept only `tags`, so capability
/// coalescing and per-model defaults were unreachable outside the proxy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelContext {
    /// Stored capability bitfield — drives request-side transforms.
    pub capabilities: ModelCapabilities,
    /// `format:*` tags — drives response-stream parser selection.
    pub tags: Vec<String>,
    /// Per-model inference defaults to merge into each request.
    pub inference_defaults: Option<InferenceConfig>,
}

impl ModelContext {
    /// The zeroed context: empty capabilities so every transform is a no-op,
    /// empty tags so the identity passthrough parser is selected, and no
    /// per-model defaults.
    ///
    /// This is the conservative fallback used whenever the model cannot be
    /// resolved — an unresolvable model must never block a request, only lose
    /// its model-specific handling.
    #[must_use]
    pub fn passthrough() -> Self {
        Self::default()
    }
}

impl From<&ModelSummary> for ModelContext {
    fn from(summary: &ModelSummary) -> Self {
        Self {
            capabilities: summary.capabilities,
            tags: summary.tags.clone(),
            inference_defaults: summary.inference_defaults.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_is_inert() {
        let ctx = ModelContext::passthrough();
        assert!(ctx.capabilities.is_empty());
        assert!(ctx.tags.is_empty());
        assert!(ctx.inference_defaults.is_none());
    }

    #[test]
    fn from_summary_carries_all_three_fields() {
        let mut summary = super::super::tests_support::summary();
        summary.capabilities = ModelCapabilities::REQUIRES_STRICT_TURNS;
        summary.tags = vec!["format:qwen".to_string()];
        summary.inference_defaults = Some(InferenceConfig {
            temperature: Some(0.5),
            ..Default::default()
        });

        let ctx = ModelContext::from(&summary);
        assert_eq!(ctx.capabilities, ModelCapabilities::REQUIRES_STRICT_TURNS);
        assert_eq!(ctx.tags, vec!["format:qwen".to_string()]);
        assert_eq!(
            ctx.inference_defaults.and_then(|c| c.temperature),
            Some(0.5)
        );
    }
}
