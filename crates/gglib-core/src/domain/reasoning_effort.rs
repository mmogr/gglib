//! The `reasoning_effort` level — how hard the model is *asked* to think.
//!
//! Kept out of [`inference`](super::inference) deliberately. This is not a
//! sampler and it does not behave like one: it is a string that a chat
//! template may read at render time, its level vocabulary is per-template
//! folklore ([ADR 0007] finding 3), and the whole reason it exists as a type
//! rather than a `String` is a governance argument that needs room to be
//! written down. `inference.rs` is already the largest file in this module and
//! the ladder is the thing it should be about; [`template_caps`] (the other
//! half of ADR 0007) set the same precedent one PR ago.
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
//! [`template_caps`]: super::template_caps

use std::fmt;

use serde::{Deserialize, Serialize};

/// A reasoning-effort level gglib is willing to put on the wire.
///
/// # This enum is where gglib is deliberately stricter than upstream
///
/// [`InferenceConfig::extract_client_sampling`]'s coercion doctrine is "accept
/// what upstream accepts and reject what upstream rejects, so gglib never
/// becomes the stricter of the two". A closed enum breaks that rule for this
/// one field, on purpose, and the exception is argued rather than assumed —
/// see [`InferenceConfig::reasoning_effort`], which carries the argument.
///
/// # The levels
///
/// The six are llama-server's own documented set, taken from its
/// `--reasoning-effort` help text on the pinned build. They are *offered*
/// levels, not honoured ones: a template that never branches on the variable
/// ignores every one of them, and even a template that reads it may only act
/// on some (upstream's own tests show DeepSeek-V4 rendering something special
/// for `"max"` and nothing at all for `"high"` or `"low"` — ADR 0007
/// finding 3).
///
/// # `"none"` is not here, and its absence is the decision
///
/// llama-server accepts `reasoning_effort: "none"` and treats it specially: it
/// **erases** the kwarg rather than passing it through. On `gpt-oss` the
/// template's own `{%- set reasoning_effort = "medium" %}` fallback then
/// fills the hole, so `"none"` yields **medium** thinking — confirmed live
/// against the pinned binary, not inferred. Offering it as a level would ship
/// a control whose most obvious value does the opposite of what it reads as.
/// "Stop thinking" is
/// [`reasoning_budget_tokens: 0`](super::InferenceConfig#structfield.reasoning_budget_tokens),
/// which is sampler-enforced and range-validated upstream. See ADR 0007
/// finding 4 and decision 4.
///
/// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
/// [`InferenceConfig::extract_client_sampling`]: super::InferenceConfig::extract_client_sampling
/// [`InferenceConfig::reasoning_effort`]: super::InferenceConfig#structfield.reasoning_effort
/// [`InferenceConfig::reasoning_budget_tokens`]: super::InferenceConfig#structfield.reasoning_budget_tokens
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    /// The least thinking the template offers.
    Minimal,
    /// Below the template's own middle setting.
    Low,
    /// The middle setting, and the value `gpt-oss`'s template falls back to on
    /// its own when no kwarg arrives.
    Medium,
    /// Above the middle setting.
    High,
    /// Serialises as `xhigh`, one word — *not* `x_high`. The wire spelling is
    /// llama-server's, and `serde(rename_all = "lowercase")` lower-cases the
    /// variant name whole rather than splitting on the case boundary the way
    /// `snake_case` would. `the_wire_spelling_of_every_level_is_pinned` fails
    /// if that ever changes.
    XHigh,
    /// The most thinking the template offers.
    Max,
}

impl ReasoningEffort {
    /// Every level, weakest first.
    ///
    /// The order is the `--reasoning-effort` help text's, which is also the
    /// only ordering the levels have — nothing in llama.cpp compares them, and
    /// a template is free to treat `low` and `high` identically.
    pub const ALL: [Self; 6] = [
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
    ];

    /// The wire spelling, identical to what `serde` emits.
    ///
    /// Both spellings exist because both are needed — `serde` writes the
    /// request body, and this reads a client's string without a round trip
    /// through `serde_json` — and they are pinned equal by test rather than by
    /// hope.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }

    /// Read a level from a client's string, or `None` if it is not one.
    ///
    /// Case-insensitive, because the levels are a closed vocabulary and
    /// `"HIGH"` can only have meant one thing. Note what is *not* here:
    /// `"none"` is not a level (see the type docs), so it lands in the `None`
    /// arm along with `"banana"` and is reported as a rejection by the caller.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|level| level.as_str().eq_ignore_ascii_case(s))
    }

    /// The accepted levels, rendered for an error message.
    #[must_use]
    pub fn wire_vocabulary() -> String {
        Self::ALL
            .iter()
            .map(|level| level.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[path = "reasoning_effort_tests.rs"]
mod reasoning_effort_tests;
