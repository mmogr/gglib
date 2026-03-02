//! Context-budget pruning for the agentic loop.
//!
//! Long agentic runs accumulate tool messages that can exceed the LLM's context
//! window.  This module trims the conversation history when the total character
//! count exceeds [`AgentConfig::context_budget_chars`], applying two passes:
//!
//! 1. **Tool-message pruning** — keep only the most recent
//!    [`AgentConfig::prune_keep_tool_messages`] tool results and drop the
//!    corresponding `Assistant` messages whose every tool call was removed.
//! 2. **Tail pruning** — if still over budget after pass 1, keep all `System`
//!    messages and the trailing [`AgentConfig::prune_keep_tail_messages`]
//!    non-system messages.
//!
//! This is a port of `pruneForContextBudget` from
//! `src/hooks/useGglibRuntime/streamAgentChat.ts` (previously `agentLoop.ts`).

use std::collections::HashSet;

use gglib_core::{AgentConfig, AgentMessage};
use tracing::{debug, warn};

// =============================================================================
// Public API
// =============================================================================

/// Total estimated character count across all messages.
pub fn total_chars(messages: &[AgentMessage]) -> usize {
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
pub fn prune_for_budget(
    messages: Vec<AgentMessage>,
    config: &AgentConfig,
) -> Vec<AgentMessage> {
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

/// Pass 2 of the pruning algorithm: emergency tail-prune.
///
/// Keeps all [`AgentMessage::System`] messages (hoisted to the front) and the
/// last [`AgentConfig::prune_keep_tail_messages`] non-system messages.
///
/// Called by [`prune_for_budget`] when Pass 1 alone was insufficient.
/// See the `# Warning` note on that function for the System message reordering
/// behaviour.
fn prune_tail(
    messages: Vec<AgentMessage>,
    config: &AgentConfig,
) -> Vec<AgentMessage> {
    let before_count = messages.len();
    let (system, non_system): (Vec<AgentMessage>, Vec<AgentMessage>) = messages
        .into_iter()
        .partition(|m| matches!(m, AgentMessage::System { .. }));

    let tail_start = non_system
        .len()
        .saturating_sub(config.prune_keep_tail_messages);
    let result: Vec<AgentMessage> = system
        .into_iter()
        .chain(non_system.into_iter().skip(tail_start))
        .collect();

    warn!(
        before = before_count,
        after = result.len(),
        dropped = before_count.saturating_sub(result.len()),
        "context-pruning Pass 2 (emergency tail-prune) triggered: \
         System messages hoisted to front, oldest non-system messages dropped"
    );

    result
}

/// Pass 1 of the pruning algorithm: drop old tool messages and orphaned
/// assistant messages to reclaim context budget.
///
/// Keeps the most recent [`AgentConfig::prune_keep_tool_messages`] `Tool`
/// messages and strips any `Assistant` messages whose every tool-call reference
/// was removed.  `Assistant` messages that still have at least one surviving
/// call are retained with only those surviving calls listed.
fn prune_tool_messages(
    messages: Vec<AgentMessage>,
    config: &AgentConfig,
) -> Vec<AgentMessage> {
    let before_count = messages.len();
    // Collect the tool_call_ids of the tool results we intend to keep
    // (the last KEEP_LAST_TOOL_MESSAGES Tool messages, in reverse order).
    let kept_tool_call_ids: HashSet<String> = messages
        .iter()
        .rev()
        .filter_map(|m| {
            if let AgentMessage::Tool { tool_call_id, .. } = m {
                Some(tool_call_id.clone())
            } else {
                None
            }
        })
        .take(config.prune_keep_tool_messages)
        .collect();

    // Replace retain() with filter_map so we can also strip the pruned call
    // IDs from retained assistant messages — leaving an assistant message that
    // references tc2/tc3 when only tc1's result survived would confuse the LLM.
    let result: Vec<AgentMessage> = messages
        .into_iter()
        .filter_map(|m| {
            match m {
                // Drop a Tool message if its id is not in the retained set.
                AgentMessage::Tool {
                    ref tool_call_id, ..
                } if !kept_tool_call_ids.contains(tool_call_id) => None,

                // For an Assistant message with tool calls: keep only if at least
                // one call survives, but also strip the pruned call IDs so the
                // context never contains references to missing tool results.
                AgentMessage::Assistant { content } => {
                    match content.tool_calls() {
                        None => Some(AgentMessage::Assistant { content }),
                        Some(calls) => {
                            let retained_calls: Vec<_> = calls
                                .iter()
                                .filter(|c| kept_tool_call_ids.contains(&c.id))
                                .cloned()
                                .collect();
                            if retained_calls.is_empty() {
                                None
                            } else {
                                Some(AgentMessage::Assistant {
                                    content: content.with_replaced_tool_calls(retained_calls),
                                })
                            }
                        }
                    }
                }

                // System and User messages are always kept.
                other => Some(other),
            }
        })
        .collect();

    let after_count: usize = result.len();
    let dropped = before_count.saturating_sub(after_count);
    if dropped > 0 {
        debug!(
            before = before_count,
            after = after_count,
            dropped,
            "context-pruning Pass 1 (tool-message pruning): dropped old tool/assistant messages"
        );
    }
    result
}
