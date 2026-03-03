//! Pass 1: drop old tool messages and orphaned assistant messages to reclaim
//! context budget.
//!
//! Keeps the most recent [`AgentConfig::prune_keep_tool_messages`] `Tool`
//! messages and strips any `Assistant` messages whose every tool-call reference
//! was removed.  `Assistant` messages that still have at least one surviving
//! call are retained with only those surviving calls listed.

use std::collections::HashSet;

use gglib_core::{AgentConfig, AgentMessage};
use tracing::debug;

/// Drop old tool messages and orphaned assistant messages.
///
/// Keeps the most recent [`AgentConfig::prune_keep_tool_messages`] `Tool`
/// messages.  `Assistant` messages whose *every* tool-call reference was
/// pruned are dropped entirely; those with at least one surviving call are
/// kept but have pruned call IDs stripped.
pub(super) fn prune_tool_messages(
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
                AgentMessage::Assistant { content } => match content.tool_calls() {
                    None => Some(AgentMessage::Assistant { content }),
                    Some(calls) => {
                        let retained_calls: Vec<_> = calls
                            .iter()
                            .filter(|c| kept_tool_call_ids.contains(&c.id))
                            .cloned()
                            .collect();
                        if retained_calls.is_empty() {
                            // All tool calls were pruned from this assistant
                            // message.  The text content (if any) is also
                            // dropped because keeping text that references
                            // calls whose results are absent would confuse the
                            // model — it would see reasoning about tool outputs
                            // it never received.
                            None
                        } else {
                            Some(AgentMessage::Assistant {
                                content: content.with_replaced_tool_calls(retained_calls),
                            })
                        }
                    }
                },

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
