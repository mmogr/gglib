//! A chat's Thinking choice: what a turn says of it, what the chat
//! remembers, and the one rule every door reads a turn by.
//!
//! The rule is [`settle`], read at a paired device's turn, the page's turn
//! on a far machine (which arrives there as a device's), this machine's own
//! run, and the CLI's chat, where `--thinking` is what the turn says.
//! A turn says `off`, `default` or nothing. `off` runs it with a thinking
//! budget of `0` and the chat remembers; `default` runs it with the
//! request's own budget and the chat forgets; nothing runs it as the chat
//! remembers, and a remembered `off` beats the request's own budget.
//!
//! Kept apart from the two reasoning controls a request carries
//! ([`InferenceConfig::reasoning_budget_tokens`] and
//! [`ReasoningEffort`]): those are one request's own, and are never
//! remembered. This is the chat's, said on a turn and kept in its
//! [`ConversationSettings`].
//!
//! [`InferenceConfig::reasoning_budget_tokens`]: super::InferenceConfig#structfield.reasoning_budget_tokens
//! [`ReasoningEffort`]: super::ReasoningEffort
//! [`ConversationSettings`]: super::chat::ConversationSettings

use serde::{Deserialize, Serialize};

/// Whether a chat's turns run with the model's thinking switched off.
///
/// On the wire `"off"` and `"default"`. A turn says one only when the user
/// changes it; a turn that says neither runs as its chat remembers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Thinking {
    /// The turn runs with a thinking budget of `0`, and the chat remembers
    /// it. The only choice a chat ever stores.
    Off,
    /// The model thinks as it would with nothing said: the chat forgets
    /// `off`. Stored as an absent key, never as this word.
    Default,
}

/// The thinking budget that switches a model's thinking off.
const OFF_BUDGET: i32 = 0;

/// What a chat is to remember once a run starts: `None` writes nothing,
/// `Some(choice)` sets what it remembers to `choice` (`Some(None)` forgets).
pub type Remember = Option<Option<Thinking>>;

/// A turn's Thinking choice, settled against its chat's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settled {
    /// The thinking budget the run uses, as `reasoning_budget_tokens`.
    pub budget: Option<i32>,
    /// What the chat is to remember, written when the run starts.
    pub remember: Remember,
}

/// Settle what a turn `said` against what its chat `remembered` and the
/// request's `own_budget`, which a device's turn never has.
///
/// Only `off` is ever remembered, so a write is asked for only when it
/// changes that: `off` on a chat that does not already remember it, and
/// `default` on one that remembers anything.
#[must_use]
pub fn settle(
    said: Option<Thinking>,
    remembered: Option<Thinking>,
    own_budget: Option<i32>,
) -> Settled {
    let off = Some(Thinking::Off);
    match said {
        Some(Thinking::Off) => Settled {
            budget: Some(OFF_BUDGET),
            remember: (remembered != off).then_some(off),
        },
        Some(Thinking::Default) => Settled {
            budget: own_budget,
            remember: remembered.is_some().then_some(None),
        },
        None if remembered == off => Settled {
            budget: Some(OFF_BUDGET),
            remember: None,
        },
        None => Settled {
            budget: own_budget,
            remember: None,
        },
    }
}

#[cfg(test)]
#[path = "thinking_tests.rs"]
mod thinking_tests;
