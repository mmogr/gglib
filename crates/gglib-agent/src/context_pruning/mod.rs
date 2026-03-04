//! Context-budget pruning for the agentic loop.
//!
//! Long agentic runs accumulate tool messages that can exceed the LLM's context
//! window.  This module trims the conversation history when the total character
//! count exceeds [`AgentConfig::context_budget_chars`], applying two passes:
//!
//! 1. **Tool-message pruning** ([`tool_pruning`]) — keep only the most recent
//!    [`AgentConfig::prune_keep_tool_messages`] tool results and drop the
//!    corresponding `Assistant` messages whose every tool call was removed.
//! 2. **Tail pruning** ([`tail_pruning`]) — if still over budget after pass 1,
//!    keep all `System` messages and the trailing
//!    [`AgentConfig::prune_keep_tail_messages`] non-system messages.

mod tail_pruning;
mod tool_pruning;

#[cfg(test)]
mod tests;

use gglib_core::{AgentConfig, AgentMessage};

use tail_pruning::prune_tail;
use tool_pruning::prune_tool_messages;

// =============================================================================
// Public API
// =============================================================================

/// Total estimated character count across all messages.
fn total_chars(messages: &[AgentMessage]) -> usize {
    messages.iter().map(AgentMessage::char_count).sum()
}

/// Prune `messages` so that the total character count fits within the configured
/// budget.  Returns `messages` unchanged if it is already within budget.
///
/// # Algorithm
///
/// See module-level documentation for the two-pass algorithm.
///
/// # Warning — Pass 2 reorders `System` messages
///
/// If Pass 1 alone is insufficient, Pass 2 partitions the message list into
/// `System` and non-`System` groups.  All `System` messages are moved to the
/// **front** regardless of their original positions, followed by the retained
/// non-system tail.  Most LLM APIs expect system prompts at the head of the
/// context, so this is intentional — but callers should be aware that
/// interleaved system prompts will no longer appear at their original
/// positions within the non-system flow after Pass 2 runs.
pub fn prune_for_budget(messages: Vec<AgentMessage>, config: &AgentConfig) -> Vec<AgentMessage> {
    let budget = config.context_budget_chars;

    if total_chars(&messages) <= budget {
        return messages;
    }

    // ---- Pass 1: drop old tool messages and orphaned assistant messages -----
    let messages = prune_tool_messages(messages, config);

    if total_chars(&messages) <= budget {
        return messages;
    }

    prune_tail(messages, config)
}
