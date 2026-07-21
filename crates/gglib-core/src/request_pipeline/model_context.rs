//! The resolved per-model context every request pipeline is built from.

use super::truncation::CHARS_PER_TOKEN_APPROX;
use crate::domain::{InferenceConfig, ModelCapabilities};
use crate::ports::ModelSummary;

/// Everything a request pipeline needs to know about the target model,
/// gathered in a single catalog round-trip.
///
/// The four fields feed four different stages, which is why they travel
/// together rather than being looked up where each is needed:
///
/// * [`capabilities`](Self::capabilities) — request-side transforms
///   (strict-turn coalescing and friends).
/// * [`tags`](Self::tags) — response-stream parser selection.
/// * [`inference_defaults`](Self::inference_defaults) — the per-model layer of
///   the sampling hierarchy.
/// * [`context_length`](Self::context_length) — the history-truncation budget.
///
/// Before this type was shared, the proxy resolved all of them while every
/// other surface resolved the same row and kept only `tags`, so capability
/// coalescing and per-model defaults were unreachable outside the proxy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelContext {
    /// Stored capability bitfield — drives request-side transforms.
    pub capabilities: ModelCapabilities,
    /// `format:*` tags — drives response-stream parser selection.
    pub tags: Vec<String>,
    /// Per-model inference defaults to merge into each request.
    pub inference_defaults: Option<InferenceConfig>,
    /// Maximum context the model supports, in tokens — the history-truncation
    /// budget for every surface that cannot measure a live serving context.
    pub context_length: Option<u64>,
}

impl ModelContext {
    /// The zeroed context: empty capabilities so every transform is a no-op,
    /// empty tags so the identity passthrough parser is selected, no per-model
    /// defaults, and no truncation budget.
    ///
    /// This is the conservative fallback used whenever the model cannot be
    /// resolved — an unresolvable model must never block a request, only lose
    /// its model-specific handling.
    #[must_use]
    pub fn passthrough() -> Self {
        Self::default()
    }

    /// The history-truncation budget in characters, from the model's own
    /// capacity: [`context_length`](Self::context_length) tokens converted at
    /// [`CHARS_PER_TOKEN_APPROX`].
    ///
    /// `None` when the context size is unknown, which
    /// [`apply`](super::apply) reads as *do not truncate*. Guessing a budget
    /// for an unresolvable model would risk rejecting a request over a number
    /// nobody actually knows; losing model-specific handling is the whole
    /// fallback policy of this module.
    ///
    /// Callers that know the **live** serving context — the proxy, which also
    /// learns a per-model chars-per-token ratio from observed usage frames —
    /// compute a better number and pass that instead. This is the answer for
    /// everyone else.
    #[must_use]
    pub fn context_budget_chars(&self) -> Option<usize> {
        let tokens = usize::try_from(self.context_length?).ok()?;
        Some(tokens.saturating_mul(CHARS_PER_TOKEN_APPROX))
    }
}

impl From<&ModelSummary> for ModelContext {
    fn from(summary: &ModelSummary) -> Self {
        Self {
            capabilities: summary.capabilities,
            tags: summary.tags.clone(),
            inference_defaults: summary.inference_defaults.clone(),
            context_length: summary.context_length,
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
        assert!(ctx.context_length.is_none());
    }

    #[test]
    fn from_summary_carries_every_field() {
        let mut summary = super::super::tests_support::summary();
        summary.capabilities = ModelCapabilities::REQUIRES_STRICT_TURNS;
        summary.tags = vec!["format:qwen".to_string()];
        summary.inference_defaults = Some(InferenceConfig {
            temperature: Some(0.5),
            ..Default::default()
        });
        summary.context_length = Some(32_768);

        let ctx = ModelContext::from(&summary);
        assert_eq!(ctx.capabilities, ModelCapabilities::REQUIRES_STRICT_TURNS);
        assert_eq!(ctx.tags, vec!["format:qwen".to_string()]);
        assert_eq!(
            ctx.inference_defaults.and_then(|c| c.temperature),
            Some(0.5)
        );
        assert_eq!(ctx.context_length, Some(32_768));
    }

    /// The budget scales with the model rather than sitting on a shared floor:
    /// a small-context model gets a small one, a large-context model a large.
    #[test]
    fn the_budget_scales_with_the_model() {
        let small = ModelContext {
            context_length: Some(4_096),
            ..ModelContext::passthrough()
        };
        let large = ModelContext {
            context_length: Some(262_144),
            ..ModelContext::passthrough()
        };

        assert_eq!(small.context_budget_chars(), Some(16_384));
        assert_eq!(large.context_budget_chars(), Some(1_048_576));
    }

    /// An unresolvable model must not be handed a guessed budget — `None` means
    /// "do not truncate", not "truncate at zero".
    #[test]
    fn an_unknown_context_length_yields_no_budget() {
        assert_eq!(ModelContext::passthrough().context_budget_chars(), None);
    }
}
