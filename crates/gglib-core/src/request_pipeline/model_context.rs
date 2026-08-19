//! The resolved per-model context every request pipeline is built from.

use super::truncation::CHARS_PER_TOKEN_APPROX;
use crate::domain::{
    DefaultsOrigin, DialectSpec, InferenceConfig, ModelCapabilities, TemplateCaps,
};
use crate::normalize::registry::dialect_for_tags;
use crate::ports::ModelSummary;

/// Everything a request pipeline needs to know about the target model,
/// gathered in a single catalog round-trip.
///
/// The fields feed the resolution and shaping stages, which is why they
/// travel together rather than being looked up where each is needed:
///
/// * [`capabilities`](Self::capabilities) — request-side transforms
///   (strict-turn coalescing and friends).
/// * [`dialect`](Self::dialect) — response-stream parser selection and
///   decode-time grammar origination.
/// * [`tags`](Self::tags) — the sampling floor (via the `reasoning` tag) and
///   launch narration.
/// * [`inference_defaults`](Self::inference_defaults) — the per-model layer of
///   the sampling hierarchy.
/// * [`defaults_origin`](Self::defaults_origin) — which rung
///   [`inference_defaults`](Self::inference_defaults) occupies in that
///   hierarchy.
/// * [`context_length`](Self::context_length) — the history-truncation budget.
///
/// Before this type was shared, the proxy resolved all of them while every
/// other surface resolved the same row and kept only `tags`, so capability
/// coalescing and per-model defaults were unreachable outside the proxy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelContext {
    /// Stored capability bitfield — drives request-side transforms.
    pub capabilities: ModelCapabilities,
    /// The model's tags — the sampling floor (`reasoning`) and narration.
    pub tags: Vec<String>,
    /// Resolved tool-call dialect — drives response-stream parser selection
    /// and decode-time grammar origination.
    ///
    /// Populated from the model's persisted spec when one exists, else from
    /// the `format:*` tag fallback (see [`From<&ModelSummary>`]); `None`
    /// selects the identity passthrough parser.
    pub dialect: Option<DialectSpec>,
    /// Per-model inference defaults to merge into each request.
    pub inference_defaults: Option<InferenceConfig>,
    /// Whether [`inference_defaults`](Self::inference_defaults) was set by
    /// the user or auto-detected. See [`DefaultsOrigin`].
    pub defaults_origin: Option<DefaultsOrigin>,
    /// Maximum context the model supports, in tokens — the history-truncation
    /// budget for every surface that cannot measure a live serving context.
    pub context_length: Option<u64>,
    /// llama-server's template-capability self-report, when a launch has
    /// recorded one (ADR 0007).
    ///
    /// `None` — on a passthrough context *or* a resolved row nobody has
    /// launched yet — means "never observed", which per decision 3 licenses
    /// nothing: unknown never gates. Nothing consumes this yet; the effort
    /// gate arrives in a later PR of the arc.
    pub template_caps: Option<TemplateCaps>,
    /// Whether this context came from an actual catalog row.
    ///
    /// `false` for [`passthrough`](Self::passthrough) — the fallback for
    /// unknown or unresolvable models. Transforms that act on the *absence*
    /// of a capability (tool stripping) must check this: an empty bitfield on
    /// a passthrough context means "nobody knows", not "the model can't".
    pub catalog_resolved: bool,
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
    /// [`apply`](super::apply()) reads as *do not truncate*. Guessing a budget
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
            // The single back-compat point for dialect resolution: a
            // persisted spec wins; rows that predate specs (or whose spec
            // could not be derived) fall back to their `format:*` tag.
            // Every surface builds its context here, so all of them
            // inherit the fallback.
            dialect: summary
                .dialect
                .clone()
                .or_else(|| dialect_for_tags(&summary.tags)),
            inference_defaults: summary.inference_defaults.clone(),
            defaults_origin: summary.defaults_origin,
            context_length: summary.context_length,
            template_caps: summary.template_caps.clone(),
            catalog_resolved: true,
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
        summary.defaults_origin = Some(DefaultsOrigin::AutoDetected);
        summary.context_length = Some(32_768);
        summary.template_caps = Some(TemplateCaps {
            supports_reasoning_effort: Some(true),
            ..TemplateCaps::default()
        });

        let ctx = ModelContext::from(&summary);
        assert_eq!(
            ctx.template_caps
                .as_ref()
                .and_then(|c| c.supports_reasoning_effort),
            Some(true)
        );
        assert_eq!(ctx.capabilities, ModelCapabilities::REQUIRES_STRICT_TURNS);
        assert_eq!(ctx.tags, vec!["format:qwen".to_string()]);
        assert_eq!(
            ctx.dialect, None,
            "an unrecognized tag maps to no dialect, not a guessed one"
        );
        assert_eq!(
            ctx.inference_defaults.and_then(|c| c.temperature),
            Some(0.5)
        );
        assert_eq!(ctx.defaults_origin, Some(DefaultsOrigin::AutoDetected));
        assert_eq!(ctx.context_length, Some(32_768));
    }

    /// Legacy catalog rows: a `format:qwen-xml` tag with no persisted spec
    /// resolves to the builtin — the permanent back-compat path.
    #[test]
    fn a_format_tag_without_a_spec_falls_back_to_the_builtin() {
        let mut summary = super::super::tests_support::summary();
        summary.tags = vec![crate::normalize::tags::FORMAT_QWEN_XML.to_owned()];
        summary.dialect = None;

        let ctx = ModelContext::from(&summary);
        assert_eq!(ctx.dialect, Some(DialectSpec::qwen_xml()));
    }

    /// A persisted spec always beats the tag fallback — the tag may be
    /// stale, the spec is what detection actually derived.
    #[test]
    fn a_persisted_spec_wins_over_the_tag_fallback() {
        let derived = DialectSpec {
            tool_open: "«TC»".to_owned(),
            tool_close: "«/TC»".to_owned(),
            ..DialectSpec::qwen_xml()
        };
        let mut summary = super::super::tests_support::summary();
        summary.tags = vec![crate::normalize::tags::FORMAT_QWEN_XML.to_owned()];
        summary.dialect = Some(derived.clone());

        let ctx = ModelContext::from(&summary);
        assert_eq!(ctx.dialect, Some(derived));
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
