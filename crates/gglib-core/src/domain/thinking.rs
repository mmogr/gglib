//! A chat's Thinking choice: what a turn says of it, and what the chat
//! remembers.
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
