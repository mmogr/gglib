//! A change to some of a conversation's settings.
//!
//! Its model, its tools and its tool limits: each field sent replaces its
//! setting, and one left out keeps it. `PUT /api/conversations/{id}` reads
//! it, and [`ChatHistoryService::update_conversation`] writes it.
//!
//! [`ChatHistoryService::update_conversation`]: crate::services::ChatHistoryService::update_conversation

use serde::{Deserialize, Serialize};

use super::agent::config::{
    MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING, MIN_TOOL_TIMEOUT_MS,
};
use super::chat::ConversationSettings;
use super::machine::ModelRef;

/// The settings of a conversation's runs that a client may change.
///
/// Each field that is present replaces its setting, and one left out keeps
/// it. A key this does not name is refused, so a misspelt one is not
/// dropped without a word.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct ConversationSettingsPatch {
    /// The model the conversation's next turn runs on, by its machine. One
    /// of this machine's must be in its catalogue, and a conversation's
    /// machine is fixed: a model of another machine than the one it ran on
    /// is refused.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
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
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<usize>,
    /// How long one tool call may run, in milliseconds, from
    /// [`MIN_TOOL_TIMEOUT_MS`] to [`MAX_TOOL_TIMEOUT_MS_CEILING`].
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_timeout_ms: Option<u64>,
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
        if let Some(n) = self.max_parallel
            && !(1..=MAX_PARALLEL_TOOLS_CEILING).contains(&n)
        {
            return Some(format!(
                "max_parallel must be from 1 to {MAX_PARALLEL_TOOLS_CEILING}, not {n}"
            ));
        }
        if let Some(ms) = self.tool_timeout_ms
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
    /// not known here. Tools switched back on are stored as no `no_tools`,
    /// as a conversation that never switched them off stores them.
    pub fn apply(self, settings: &mut ConversationSettings, model_name: Option<String>) {
        if let Some(model) = self.model {
            settings.model = Some(model);
            settings.model_name = model_name;
        }
        if let Some(tools) = self.tools {
            settings.tools = tools;
        }
        if let Some(no_tools) = self.no_tools {
            settings.no_tools = no_tools.then_some(true);
        }
        if let Some(n) = self.max_parallel {
            settings.max_parallel = Some(n);
        }
        if let Some(ms) = self.tool_timeout_ms {
            settings.tool_timeout_ms = Some(ms);
        }
    }
}

#[cfg(test)]
#[path = "conversation_patch_tests.rs"]
mod conversation_patch_tests;
