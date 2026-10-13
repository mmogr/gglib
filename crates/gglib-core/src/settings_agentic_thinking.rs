//! How much a turn that carries tools may think, on the proxy.
//!
//! A `#[path]` sibling of `settings.rs`, as `settings_loop_guard.rs` is: the
//! type is a settings value with a wire spelling and a generated TypeScript
//! mirror, and it keeps its one rule, [`AgenticThinking::budget_for`], with it.
//!
//! [ADR 0019] holds the measurement behind the default and what would reopen
//! it.
//!
//! [ADR 0019]: https://github.com/mmogr/gglib/blob/main/docs/adr/0019-a-tool-turn-acts.md

use serde::{Deserialize, Serialize};

use crate::request_pipeline::LastSpeaker;

/// The reasoning budget, in tokens, [`AgenticThinking::Brief`] gives a turn
/// that carries tools.
///
/// No run has measured this number: [ADR 0019] records it as a choice.
///
/// [ADR 0019]: https://github.com/mmogr/gglib/blob/main/docs/adr/0019-a-tool-turn-acts.md
pub const AGENTIC_BRIEF_BUDGET_TOKENS: i32 = 512;

/// The `reasoning_budget_message` sent beside a budget above zero:
/// llama-server inserts it before the end tag it forces when the budget runs
/// out ([ADR 0019]).
///
/// [ADR 0019]: https://github.com/mmogr/gglib/blob/main/docs/adr/0019-a-tool-turn-acts.md
pub const AGENTIC_BUDGET_MESSAGE: &str = "Enough thinking. Act now.";

/// What the proxy does to the reasoning budget of a request that carries
/// tools, when nobody chose how much it thinks.
///
/// `Default::default()` is [`Default`](Self::Default), which changes nothing,
/// so a caller with no settings to follow adjusts nothing. What an install
/// with nothing stored does is a different question, answered by
/// [`Settings::effective_agentic_thinking`](super::Settings::effective_agentic_thinking):
/// [`WhenAsked`](Self::WhenAsked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum AgenticThinking {
    /// gglib changes nothing: the turn thinks as the model and its resolved
    /// sampling have it.
    #[default]
    Default,
    /// Every turn that carries tools gets a reasoning budget of `0`.
    Off,
    /// Every turn that carries tools gets a budget of
    /// [`AGENTIC_BRIEF_BUDGET_TOKENS`], with [`AGENTIC_BUDGET_MESSAGE`].
    Brief,
    /// A turn that carries tools gets a budget of `0` unless its last message
    /// is the user's, and a turn whose last message is the user's is left
    /// alone.
    WhenAsked,
}

impl AgenticThinking {
    /// The budget this policy gives a turn that carries tools, by who spoke
    /// last, or `None` when it leaves the turn alone.
    ///
    /// The caller decides whether the turn carries tools and whether somebody
    /// chose how much it thinks already; this answers only what the policy
    /// would set.
    #[must_use]
    pub const fn budget_for(self, last_speaker: LastSpeaker) -> Option<i32> {
        match (self, last_speaker) {
            (Self::Default, _) | (Self::WhenAsked, LastSpeaker::Person) => None,
            (Self::Off, _) | (Self::WhenAsked, LastSpeaker::Work) => Some(0),
            (Self::Brief, _) => Some(AGENTIC_BRIEF_BUDGET_TOKENS),
        }
    }
}
