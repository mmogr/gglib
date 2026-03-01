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
//! `src/hooks/useGglibRuntime/agentLoop.ts`.

use std::collections::HashSet;

use gglib_core::{AgentConfig, AgentMessage};

// =============================================================================
// Public API
// =============================================================================

/// Estimate the Unicode scalar-value count of a single message.
///
/// Uses [`str::chars().count()`] rather than [`str::len`] (byte count) so that
/// multi-byte characters are counted as one unit, matching how LLMs typically
/// measure context length.
fn content_len(msg: &AgentMessage) -> usize {
    match msg {
        AgentMessage::System { content } | AgentMessage::User { content } => {
            content.chars().count()
        }
        AgentMessage::Assistant {
            content,
            tool_calls,
        } => {
            content.as_deref().map_or(0, |s| s.chars().count())
                + tool_calls.as_deref().map_or(0, |tc| {
                    tc.iter()
                        .map(|c| c.name.chars().count() + c.arguments.to_string().chars().count())
                        .sum()
                })
        }
        AgentMessage::Tool {
            tool_call_id,
            content,
        } => tool_call_id.chars().count() + content.chars().count(),
    }
}

/// Total estimated character count across all messages.
pub fn total_chars(messages: &[AgentMessage]) -> usize {
    messages.iter().map(content_len).sum()
}

/// Prune `messages` so that the total character count fits within the configured
/// budget.  Returns `messages` unchanged if it is already within budget.
///
/// # Algorithm
///
/// See module-level documentation for the two-pass algorithm.
pub fn prune_for_budget(
    mut messages: Vec<AgentMessage>,
    config: &AgentConfig,
) -> Vec<AgentMessage> {
    let budget = config.context_budget_chars;

    // A running total is tracked throughout pass 1 so that we avoid a second
    // O(n) scan of the filtered message list before deciding whether pass 2 is
    // needed.
    let mut running = total_chars(&messages);
    if running <= budget {
        return messages;
    }

    // ---- Pass 1: drop old tool messages and orphaned assistant messages -----

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
    messages = messages
        .into_iter()
        .filter_map(|m| {
            // Compute the size of this message before (potentially) moving it.
            let old_size = content_len(&m);
            match m {
                // Drop a Tool message if its id is not in the retained set.
                AgentMessage::Tool {
                    ref tool_call_id, ..
                } if !kept_tool_call_ids.contains(tool_call_id) => {
                    running -= old_size;
                    None
                }

                // For an Assistant message with tool calls: keep only if at least
                // one call survives, but also strip the pruned call IDs so the
                // context never contains references to missing tool results.
                AgentMessage::Assistant {
                    content,
                    tool_calls: Some(calls),
                } => {
                    let retained_calls: Vec<_> = calls
                        .into_iter()
                        .filter(|c| kept_tool_call_ids.contains(&c.id))
                        .collect();
                    if retained_calls.is_empty() {
                        running -= old_size;
                        None
                    } else {
                        let new_msg = AgentMessage::Assistant {
                            content,
                            tool_calls: Some(retained_calls),
                        };
                        // Adjust running for the difference in size when some
                        // tool calls were stripped from this assistant message.
                        running = running - old_size + content_len(&new_msg);
                        Some(new_msg)
                    }
                }

                // System, User, and Assistant-with-no-tool-calls are always kept.
                other => Some(other),
            }
        })
        .collect();

    if running <= budget {
        return messages;
    }

    // ---- Pass 2: emergency tail-prune ---------------------------------------
    // Keep all System messages at their original positions and the last
    // KEEP_TAIL_MESSAGES non-system messages.

    let (system, non_system): (Vec<AgentMessage>, Vec<AgentMessage>) = messages
        .into_iter()
        .partition(|m| matches!(m, AgentMessage::System { .. }));

    let tail_start = non_system
        .len()
        .saturating_sub(config.prune_keep_tail_messages);
    system
        .into_iter()
        .chain(non_system.into_iter().skip(tail_start))
        .collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use gglib_core::{AgentConfig, AgentMessage, ToolCall};
    use serde_json::json;

    use super::*;

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
            content: Some(s.to_owned()),
            tool_calls: None,
        }
    }
    fn assistant_with_calls(id: &str, name: &str) -> AgentMessage {
        AgentMessage::Assistant {
            content: None,
            tool_calls: Some(vec![ToolCall {
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
        let cfg = AgentConfig {
            context_budget_chars: 10_000,
            ..Default::default()
        };
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
        let cfg = AgentConfig {
            context_budget_chars: total - 1,
            ..Default::default()
        };

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
        let cfg = AgentConfig {
            context_budget_chars: total - 1,
            ..Default::default()
        };
        let result = prune_for_budget(msgs, &cfg);

        // call_0 was pruned → its matching assistant should also be gone.
        let has_call_0_assistant = result.iter().any(|m| {
            if let AgentMessage::Assistant {
                tool_calls: Some(calls),
                ..
            } = m
            {
                calls.iter().any(|c| c.id == "call_0")
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
        //
        // Scenario: assistant calls [tc_old, tc_new]. tc_old's result is older
        // than prune_keep_tool_messages=1, so it gets dropped; tc_new's result
        // is the most recent and is retained.  The assistant message should
        // survive with only [tc_new] in its tool_calls.
        let assistant_multi = AgentMessage::Assistant {
            content: None,
            tool_calls: Some(vec![
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
        let cfg = AgentConfig {
            context_budget_chars: total - 1,
            prune_keep_tool_messages: 1, // keep only tc_new
            ..Default::default()
        };

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
                if let AgentMessage::Assistant {
                    tool_calls: Some(calls),
                    ..
                } = m
                {
                    Some(calls.iter().map(|c| c.id.as_str()).collect::<Vec<_>>())
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
        // Force into pass-2 territory by using a very tight budget.
        let msgs = vec![
            system("S"),
            user("U1"),
            assistant_text(&"A".repeat(5000)),
            user("U2"),
            assistant_text(&"B".repeat(5000)),
        ];

        let cfg = AgentConfig {
            context_budget_chars: 50,
            ..Default::default()
        };
        let result = prune_for_budget(msgs, &cfg);

        // System message must survive pass 2.
        assert!(
            result
                .iter()
                .any(|m| matches!(m, AgentMessage::System { .. })),
            "system message must be preserved"
        );
        // Should have at most system + prune_keep_tail_messages items.
        assert!(result.len() <= 1 + cfg.prune_keep_tail_messages);
    }
}
