//! The capability tags gglib stores on a model, and the predicates that read
//! them.
//!
//! A capability tag is written once at import time by
//! [`GgufCapabilities::to_tags`](crate::domain::GgufCapabilities::to_tags) and
//! read afterwards to decide what gglib does differently for that model:
//! `agent` auto-enables `--jinja`, `mtp` auto-enables speculative decoding,
//! `embedding` auto-enables `--embeddings`, and `reasoning` selects a whole
//! sampling floor.
//!
//! Distinct from [`crate::normalize::tags`], which holds `format:*` tags for
//! the dialect registry. Those describe how a model's *output* is shaped; these
//! describe what the model can *do*.
//!
//! # Why the constants exist
//!
//! Same rule `normalize::tags` states: no other crate should hard-code these
//! strings. The tag is a stored value with a producer and many consumers, and a
//! typo on the consumer side is silent — a model simply stops being treated as
//! a reasoning model, with no error anywhere.
//!
//! # Why [`is_reasoning`] is a function and not an inlined `any`
//!
//! It was inlined, eight times across five crates, in two shapes. Two of them
//! were byte-identical private `is_reasoning` helpers in different crates; four
//! more were the same `any` expression written out inside a
//! [`ModelSamplingContext`](crate::domain::ModelSamplingContext) literal.
//!
//! The duplication was not the real cost. This predicate decides which floor a
//! model resolves against, and since [ADR 0003] the two floors differ in *which
//! parameters they name at all* — a reasoning model is sent `min_p: 0.0` while
//! every other model is sent no `min_p` key. So a call site that spelled the
//! predicate slightly differently would not produce a slightly different
//! number; it would produce a request with a different set of keys, resolved
//! from a different floor, and nothing would report an error.
//!
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md

/// Chain-of-thought model (Qwen3.6, DeepSeek-R1, `QwQ`, …).
///
/// Selects [`InferenceConfig::reasoning_floor`] over the neutral floor, drives
/// `--reasoning-format deepseek`, and causes
/// [`InferenceConfig::reasoning_profile`] to be written as the model's
/// auto-detected defaults at import.
///
/// [`InferenceConfig::reasoning_floor`]: crate::domain::InferenceConfig::reasoning_floor
/// [`InferenceConfig::reasoning_profile`]: crate::domain::InferenceConfig::reasoning_profile
pub const REASONING: &str = "reasoning";

/// Tool-calling model. Auto-enables `--jinja` at launch.
pub const AGENT: &str = "agent";

/// Multi-token prediction. Auto-enables `--spec-type draft-mtp`.
pub const MTP: &str = "mtp";

/// Embedding model rather than a generative one. Auto-enables `--embeddings`.
pub const EMBEDDING: &str = "embedding";

/// Vision-capable model.
pub const VISION: &str = "vision";

/// Code-specialised model.
pub const CODE: &str = "code";

/// Mixture-of-experts architecture.
pub const MOE: &str = "moe";

/// Every tag [`GgufCapabilities::to_tags`] can write.
///
/// The set a full re-detect clears before re-adding: a tag in this namespace
/// is gglib's to own, so a model that loses a capability must lose its tag
/// rather than keep a stale one. Kept beside the constants so the list cannot
/// fall behind the producer — a tag missing here survives a refresh forever,
/// which is a silent wrong answer rather than a visible failure.
///
/// [`GgufCapabilities::to_tags`]: crate::domain::GgufCapabilities::to_tags
pub const ALL: &[&str] = &[REASONING, AGENT, VISION, CODE, MOE, MTP, EMBEDDING];

/// Whether `tags` contains `tag`, case-insensitively.
///
/// Case-insensitive because tags reach the catalog from three sources — GGUF
/// capability detection, `HuggingFace` metadata, and hand edits — and only the
/// first is guaranteed to be lowercase.
#[must_use]
pub fn has(tags: &[String], tag: &str) -> bool {
    tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
}

/// Whether this model is in the reasoning class.
///
/// The single definition. See the module docs for why a second one is worse
/// than it looks.
#[must_use]
pub fn is_reasoning(tags: &[String]) -> bool {
    has(tags, REASONING)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_reasoning_tagged_model_is_in_the_reasoning_class() {
        assert!(is_reasoning(&tags(&["reasoning"])));
        assert!(is_reasoning(&tags(&["agent", "reasoning", "mtp"])));
    }

    #[test]
    fn an_untagged_model_is_not() {
        assert!(!is_reasoning(&[]));
        assert!(!is_reasoning(&tags(&["agent", "mtp"])));
    }

    /// Tags arrive from GGUF detection, `HuggingFace` metadata and hand edits,
    /// and only the first is guaranteed lowercase. Every call site this
    /// replaced already used `eq_ignore_ascii_case`; keeping that is what makes
    /// the extraction behaviour-preserving rather than a quiet tightening.
    #[test]
    fn the_match_is_case_insensitive_because_tags_arrive_from_three_sources() {
        assert!(is_reasoning(&tags(&["Reasoning"])));
        assert!(is_reasoning(&tags(&["REASONING"])));
    }

    /// A near-miss must not match. `reasoning` is a whole tag, not a prefix —
    /// `format:think-tag` and friends describe output shape and live in
    /// `normalize::tags`.
    #[test]
    fn a_tag_that_merely_contains_the_word_does_not_match() {
        assert!(!is_reasoning(&tags(&["reasoning-lite"])));
        assert!(!is_reasoning(&tags(&["pre-reasoning"])));
    }

    #[test]
    fn has_reads_any_tag_in_the_vocabulary() {
        let t = tags(&["agent", "MTP"]);
        assert!(has(&t, AGENT));
        assert!(has(&t, MTP));
        assert!(!has(&t, EMBEDDING));
    }

    /// [`ALL`] must cover everything the producer can emit.
    ///
    /// The failure this prevents is silent in both directions and neither
    /// shows up as an error: a tag the producer writes but `ALL` omits
    /// survives a full re-detect forever, so a model keeps a capability it no
    /// longer has; a tag in `ALL` that nothing writes clears a namespace gglib
    /// does not own.
    ///
    /// Asserted against a fully-capable `GgufCapabilities` rather than a hand
    /// list, so the producer itself is the reference.
    #[test]
    fn all_covers_every_tag_the_producer_can_write() {
        let caps = crate::domain::GgufCapabilities {
            flags: crate::domain::CapabilityFlags::all(),
            ..crate::domain::GgufCapabilities::empty()
        };
        let produced = caps.to_tags();

        for tag in &produced {
            assert!(
                ALL.contains(&tag.as_str()),
                "to_tags() emits {tag:?}, which ALL does not list — a model would keep it \
                 through a full re-detect"
            );
        }
        for tag in ALL {
            assert!(
                produced.iter().any(|p| p == tag),
                "ALL lists {tag:?}, which to_tags() never emits — clearing it on re-detect \
                 would drop a tag gglib does not own"
            );
        }
    }
}
