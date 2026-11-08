//! A change to some of a conversation's settings.
//!
//! Its model, its tools and its tool limits: each field sent replaces its
//! setting, one sent as `null` clears it, and one left out keeps it.
//! `PUT /api/conversations/{id}` reads it, and
//! [`ChatHistoryService::update_conversation`] writes it.
//!
//! [`ChatHistoryService::update_conversation`]: crate::services::ChatHistoryService::update_conversation

use serde::{Deserialize, Deserializer, Serialize};

use super::agent::config::{
    MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING, MIN_TOOL_TIMEOUT_MS,
};
use super::chat::ConversationSettings;
use super::machine::ModelRef;

/// The settings of a conversation's runs that a client may change.
///
/// Each field that is present replaces its setting, and one left out keeps
/// it. The model and the two limits may be sent as `null`, which clears
/// them: no model, and a run's own default limit. A key this does not name
/// is refused, so a misspelt one is not dropped without a word.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct ConversationSettingsPatch {
    /// The model the conversation's next turn runs on, by its machine. One
    /// of this machine's must be in its catalogue, and once a conversation
    /// has a saved message its machine is fixed: a model of another machine
    /// than the one it ran on is refused.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<ModelRef>", optional = nullable))]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub model: Option<Option<ModelRef>>,
    /// The tools its runs may call, by name; empty is every enabled one.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    /// Whether its runs call no tools at all.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_tools: Option<bool>,
    /// How many tool calls its runs make at once, from 1 to
    /// [`MAX_PARALLEL_TOOLS_CEILING`].
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_parallel: Option<Option<usize>>,
    /// How long one tool call may run, in milliseconds, from
    /// [`MIN_TOOL_TIMEOUT_MS`] to [`MAX_TOOL_TIMEOUT_MS_CEILING`].
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_timeout_ms: Option<Option<u64>>,
}

impl ConversationSettingsPatch {
    /// Whether this changes no setting at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Why this patch is refused, if it is: a limit outside the bounds a run
    /// holds it to. A refused patch changes nothing.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        if let Some(Some(n)) = self.max_parallel
            && !(1..=MAX_PARALLEL_TOOLS_CEILING).contains(&n)
        {
            return Some(format!(
                "max_parallel must be from 1 to {MAX_PARALLEL_TOOLS_CEILING}, not {n}"
            ));
        }
        if let Some(Some(ms)) = self.tool_timeout_ms
            && !(MIN_TOOL_TIMEOUT_MS..=MAX_TOOL_TIMEOUT_MS_CEILING).contains(&ms)
        {
            return Some(format!(
                "tool_timeout_ms must be from {MIN_TOOL_TIMEOUT_MS} to \
                 {MAX_TOOL_TIMEOUT_MS_CEILING}, not {ms}"
            ));
        }
        None
    }

    /// Make this patch to `settings`. A new model brings `model_name`, the
    /// name it goes by, in place of the old model's name: none when it is
    /// not known here, or when the model is cleared. Tools switched back on
    /// are stored as no `no_tools`, as a conversation that never switched
    /// them off stores them.
    pub fn apply(self, settings: &mut ConversationSettings, model_name: Option<String>) {
        if let Some(model) = self.model {
            settings.model = model;
            settings.model_name = model_name;
        }
        if let Some(tools) = self.tools {
            settings.tools = tools;
        }
        if let Some(no_tools) = self.no_tools {
            settings.no_tools = no_tools.then_some(true);
        }
        if let Some(n) = self.max_parallel {
            settings.max_parallel = n;
        }
        if let Some(ms) = self.tool_timeout_ms {
            settings.tool_timeout_ms = ms;
        }
    }
}

/// A field that is present, as `Some`: `Some(None)` for a `null`, which
/// clears its setting. An absent field is `None` by `#[serde(default)]`.
#[expect(
    clippy::option_option,
    reason = "the three states are the API: absent keeps, null clears, a value sets"
)]
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
#[path = "conversation_patch_tests.rs"]
mod conversation_patch_tests;
