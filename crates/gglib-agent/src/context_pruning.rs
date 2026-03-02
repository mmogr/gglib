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

/// Pass 2 of the pruning algorithm: emergency tail-prune.
///
/// Keeps all [`AgentMessage::System`] messages (hoisted to the front) and the
/// last [`AgentConfig::prune_keep_tail_messages`] non-system messages.
///
/// Called by [`prune_for_budget`] when Pass 1 alone was insufficient.
/// See the `# Warning` note on that function for the System message reordering
/// behaviour.
fn prune_tail(messages: Vec<AgentMessage>, config: &AgentConfig) -> Vec<AgentMessage> {
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
fn prune_tool_messages(messages: Vec<AgentMessage>, config: &AgentConfig) -> Vec<AgentMessage> {
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use gglib_core::{AgentConfig, AgentMessage, AssistantContent, ToolCall};
    use serde_json::json;

    use super::{prune_for_budget, total_chars};

    fn system(s: &str) -> AgentMessage {
        AgentMessage::System {
            content: s.to_owned(),
        }
    }
    fn user(s: &str) -> AgentMessage {
        AgentMessage::User {
            content: s.to_owned(),
        }
    }
    fn assistant_text(s: &str) -> AgentMessage {
        AgentMessage::Assistant {
            content: AssistantContent::Content(s.to_owned()),
        }
    }
    fn assistant_with_calls(id: &str, name: &str) -> AgentMessage {
        AgentMessage::Assistant {
            content: AssistantContent::ToolCalls(vec![ToolCall {
                id: id.to_owned(),
                name: name.to_owned(),
                arguments: json!({}),
            }]),
        }
    }
    fn tool_result(call_id: &str, content: &str) -> AgentMessage {
        AgentMessage::Tool {
            tool_call_id: call_id.to_owned(),
            content: content.to_owned(),
        }
    }

    #[test]
    fn within_budget_returns_unchanged() {
        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = 10_000;
        let msgs = vec![system("sys"), user("hi")];
        let result = prune_for_budget(msgs, &cfg);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn pass1_drops_old_tool_messages_first() {
        // Build messages that exceed the budget. 11 tool results → only last 10 kept.
        let mut msgs = vec![system("sys")];
        for i in 0..11 {
            let id = format!("call_{i}");
            msgs.push(assistant_with_calls(&id, "tool"));
            msgs.push(tool_result(&id, &"x".repeat(100)));
        }

        // Budget is just barely exceeded.
        let total = total_chars(&msgs);
        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = total - 1;

        let result = prune_for_budget(msgs, &cfg);

        // The oldest tool result (call_0) should have been dropped.
        let tool_ids: Vec<_> = result
            .iter()
            .filter_map(|m| {
                if let AgentMessage::Tool { tool_call_id, .. } = m {
                    Some(tool_call_id.clone())
                } else {
                    None
                }
            })
            .collect();

        assert!(
            !tool_ids.contains(&"call_0".to_owned()),
            "oldest tool message should be pruned; kept: {tool_ids:?}"
        );
        assert!(
            tool_ids.contains(&"call_10".to_owned()),
            "newest tool message should be retained"
        );
    }

    #[test]
    fn pass1_drops_orphaned_assistant_messages() {
        // An assistant message that only references pruned tool calls should be dropped.
        let mut msgs = vec![system("sys")];
        // Add 11 call/result pairs; the oldest assistant+tool will be pruned.
        for i in 0..11 {
            let id = format!("call_{i}");
            msgs.push(assistant_with_calls(&id, "t"));
            msgs.push(tool_result(&id, &"y".repeat(100)));
        }

        let total = total_chars(&msgs);
        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = total - 1;
        let result = prune_for_budget(msgs, &cfg);

        // call_0 was pruned → its matching assistant should also be gone.
        let has_call_0_assistant = result.iter().any(|m| {
            if let AgentMessage::Assistant { content } = m {
                content
                    .tool_calls()
                    .map_or(false, |calls| calls.iter().any(|c| c.id == "call_0"))
            } else {
                false
            }
        });
        assert!(
            !has_call_0_assistant,
            "orphaned assistant message should be dropped"
        );
    }

    #[test]
    fn pass1_strips_pruned_call_ids_from_partially_surviving_assistant_message() {
        // An assistant message with TWO tool calls where only one result survives
        // pruning should be kept, but the pruned call ID must be stripped from
        // the `tool_calls` list so the LLM never sees a reference to a missing
        // tool result.
        let assistant_multi = AgentMessage::Assistant {
            content: AssistantContent::ToolCalls(vec![
                ToolCall {
                    id: "tc_old".into(),
                    name: "t".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    id: "tc_new".into(),
                    name: "t".into(),
                    arguments: json!({}),
                },
            ]),
        };
        let msgs = vec![
            system("sys"),
            assistant_multi,
            tool_result("tc_old", &"x".repeat(100)),
            tool_result("tc_new", &"y".repeat(100)),
        ];

        let total = total_chars(&msgs);
        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = total - 1;
        cfg.prune_keep_tool_messages = 1; // keep only tc_new

        let result = prune_for_budget(msgs, &cfg);

        // tc_old's Tool message must be gone.
        let tool_ids: Vec<_> = result
            .iter()
            .filter_map(|m| {
                if let AgentMessage::Tool { tool_call_id, .. } = m {
                    Some(tool_call_id.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            !tool_ids.contains(&"tc_old"),
            "tc_old result should be pruned"
        );
        assert!(tool_ids.contains(&"tc_new"), "tc_new result should be kept");

        // The assistant message must survive but with tc_old stripped out.
        let assistant_calls: Vec<_> = result
            .iter()
            .filter_map(|m| {
                if let AgentMessage::Assistant { content } = m {
                    content
                        .tool_calls()
                        .map(|calls| calls.iter().map(|c| c.id.as_str()).collect::<Vec<_>>())
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        assert!(
            !assistant_calls.contains(&"tc_old"),
            "pruned call id must be stripped from assistant message"
        );
        assert!(
            assistant_calls.contains(&"tc_new"),
            "surviving call id must remain in assistant message"
        );
    }

    #[test]
    fn pass2_keeps_system_and_tail() {
        let msgs = vec![
            system("S"),
            user("U1"),
            assistant_text(&"A".repeat(5_000)),
            user("U-recent"),
            assistant_text("Best answer."),
        ];

        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = 50;
        cfg.prune_keep_tail_messages = 2;
        let result = prune_for_budget(msgs, &cfg);

        assert!(
            result
                .iter()
                .any(|m| matches!(m, AgentMessage::System { .. })),
            "system message must be preserved"
        );
        assert!(result.len() <= 1 + cfg.prune_keep_tail_messages);
        let after_chars = total_chars(&result);
        assert!(
            after_chars <= cfg.context_budget_chars,
            "pass-2 result still exceeds budget: {after_chars} > {}",
            cfg.context_budget_chars
        );
    }

    #[test]
    fn pass2_reorders_interleaved_system_messages_to_front() {
        let msgs = vec![
            user("U1"),
            system("SYS-A"),
            assistant_text(&"A".repeat(5_000)),
            system("SYS-B"),
            user("U-recent"),
        ];

        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = 50;
        cfg.prune_keep_tail_messages = 1;

        let result = prune_for_budget(msgs, &cfg);

        let system_contents: Vec<_> = result
            .iter()
            .filter_map(|m| {
                if let AgentMessage::System { content } = m {
                    Some(content.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            system_contents.len(),
            2,
            "both system messages must survive Pass 2"
        );
        assert!(system_contents.contains(&"SYS-A"), "SYS-A must be present");
        assert!(system_contents.contains(&"SYS-B"), "SYS-B must be present");

        assert!(
            matches!(&result[0], AgentMessage::System { .. }),
            "result[0] must be a System message after Pass 2 re-ordering; got {:?}",
            result[0]
        );
        assert!(
            matches!(&result[1], AgentMessage::System { .. }),
            "result[1] must be a System message after Pass 2 re-ordering; got {:?}",
            result[1]
        );

        let last_system_pos = result
            .iter()
            .rposition(|m| matches!(m, AgentMessage::System { .. }))
            .expect("at least one system message expected");
        for (i, msg) in result.iter().enumerate().take(last_system_pos) {
            assert!(
                matches!(msg, AgentMessage::System { .. }),
                "non-System message at position {i} precedes all System messages; got {:?}",
                msg
            );
        }
    }
}
