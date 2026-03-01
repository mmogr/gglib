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

/// Total estimated character count across all messages.
pub(crate) fn total_chars(messages: &[AgentMessage]) -> usize {
    messages.iter().map(AgentMessage::char_count).sum()
}

/// Prune `messages` so that the total character count fits within the configured
/// budget.  Returns `messages` unchanged if it is already within budget.
///
/// `running_chars` must be initialised by the caller to `total_chars(&messages)`
/// before the first call.  The function updates it in place as messages are
/// dropped, so the caller can maintain an accurate count across iterations
/// **without** re-scanning the full history on every call.
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
pub(crate) fn prune_for_budget(
    messages: Vec<AgentMessage>,
    config: &AgentConfig,
    running_chars: &mut usize,
) -> Vec<AgentMessage> {
    let budget = config.context_budget_chars;

    if *running_chars <= budget {
        return messages;
    }

    // ---- Pass 1: drop old tool messages and orphaned assistant messages -----
    let messages = prune_tool_messages(messages, config, running_chars);

    if *running_chars <= budget {
        return messages;
    }

    // ---- Pass 2: emergency tail-prune ---------------------------------------
    // Keep all System messages at their original positions and the last
    // KEEP_TAIL_MESSAGES non-system messages.
    //
    // NOTE: this pass re-orders the message stream.  All `System` messages are
    // moved to the **front** of the output (via `partition`) followed by the
    // retained non-system tail.  If the original conversation interleaved
    // system prompts with user/assistant turns, the relative ordering of those
    // system prompts is preserved but they will no longer appear at their
    // original positions within the non-system flow.  This is intentional —
    // most LLM APIs expect system messages at the head of the context window.

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
    // Sync the running counter after Pass 2; Pass 1 updated it incrementally
    // but the partition+skip above drops additional messages without touching
    // `running_chars`.  Without this, the next iteration sees a stale count
    // above budget and re-runs Pass 2 unnecessarily, over-pruning history.
    *running_chars = total_chars(&result);
    result
}

/// Pass 1 of the pruning algorithm: drop old tool messages and orphaned
/// assistant messages to reclaim context budget.
///
/// Keeps the most recent [`AgentConfig::prune_keep_tool_messages`] `Tool`
/// messages and strips any `Assistant` messages whose every tool-call reference
/// was removed.  `Assistant` messages that still have at least one surviving
/// call are retained with only those surviving calls listed.
///
/// `running` is updated in place so the caller can decide whether Pass 2 is
/// still needed without a separate `total_chars` scan.
fn prune_tool_messages(
    messages: Vec<AgentMessage>,
    config: &AgentConfig,
    running: &mut usize,
) -> Vec<AgentMessage> {
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
    messages
        .into_iter()
        .filter_map(|m| {
            // Compute the size of this message before (potentially) moving it.
            let old_size = m.char_count();
            match m {
                // Drop a Tool message if its id is not in the retained set.
                AgentMessage::Tool {
                    ref tool_call_id, ..
                } if !kept_tool_call_ids.contains(tool_call_id) => {
                    *running -= old_size;
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
                        *running -= old_size;
                        None
                    } else {
                        let new_msg = AgentMessage::Assistant {
                            content,
                            tool_calls: Some(retained_calls),
                        };
                        // Adjust running for the difference in size when some
                        // tool calls were stripped from this assistant message.
                        *running = *running - old_size + new_msg.char_count();
                        Some(new_msg)
                    }
                }

                // System, User, and Assistant-with-no-tool-calls are always kept.
                other => Some(other),
            }
        })
        .collect()
}



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
        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = 10_000;
        let msgs = vec![system("sys"), user("hi")];
        let mut chars = total_chars(&msgs);
        let result = prune_for_budget(msgs, &cfg, &mut chars);
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

        let mut chars = total;
        let result = prune_for_budget(msgs, &cfg, &mut chars);

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
        let mut chars = total;
        let result = prune_for_budget(msgs, &cfg, &mut chars);

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
        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = total - 1;
        cfg.prune_keep_tool_messages = 1; // keep only tc_new

        let mut chars = total;
        let result = prune_for_budget(msgs, &cfg, &mut chars);

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
        // Use a very tight budget and an explicit tail window of 2 so pass-2 is
        // forced and the outcome is deterministic.
        //
        // Character accounting (pass-2 survivors):
        //   system("S")                    →  1 char  (always kept)
        //   user("U-recent")               →  8 chars ─┐ last 2 non-system
        //   assistant_text("Best answer.") → 12 chars ─┘
        //   total = 21 ≤ 50  ✓
        let msgs = vec![
            system("S"),                      // 1 char  — always kept
            user("U1"),                       // 2 chars — outside tail, dropped
            assistant_text(&"A".repeat(5_000)), // 5 000 chars — forces pass-2
            user("U-recent"),                 // 8 chars ─┐ tail of 2
            assistant_text("Best answer."),   // 12 chars ─┘
        ];

        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = 50;
        cfg.prune_keep_tail_messages = 2;
        let mut chars = total_chars(&msgs);
        let result = prune_for_budget(msgs, &cfg, &mut chars);

        // System message must survive pass 2.
        assert!(
            result
                .iter()
                .any(|m| matches!(m, AgentMessage::System { .. })),
            "system message must be preserved"
        );
        // Should have at most system + prune_keep_tail_messages items.
        assert!(result.len() <= 1 + cfg.prune_keep_tail_messages);
        // The trimmed result must also fit inside the character budget.
        let after_chars = total_chars(&result);
        assert!(
            after_chars <= cfg.context_budget_chars,
            "pass-2 result still exceeds budget: {after_chars} > {}",
            cfg.context_budget_chars
        );
    }

    #[test]
    fn pass2_reorders_interleaved_system_messages_to_front() {
        // Build a history where System messages are **interleaved** at different
        // positions among user/assistant turns.  Pass 2 must hoist all of them
        // to the front of the output slice (preserving mutual ordering among
        // the system messages) so the LLM always sees system prompts first.
        //
        // Layout (5 messages, no tool calls so Pass 1 is a no-op):
        //   [0] User("U1")               — 2 chars
        //   [1] System("SYS-A")          — 5 chars   ← interleaved
        //   [2] Assistant("A".repeat(5_000)) — 5 000 chars  ← forces Pass 2
        //   [3] System("SYS-B")          — 5 chars   ← interleaved
        //   [4] User("U-recent")         — 8 chars   ← tail
        let msgs = vec![
            user("U1"),
            system("SYS-A"),
            assistant_text(&"A".repeat(5_000)),
            system("SYS-B"),
            user("U-recent"),
        ];

        let mut cfg = AgentConfig::default();
        cfg.context_budget_chars = 50;
        cfg.prune_keep_tail_messages = 1; // keep only "U-recent" in the non-system tail

        let mut chars = total_chars(&msgs);
        let result = prune_for_budget(msgs, &cfg, &mut chars);

        // Both system messages must be present.
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

        // The first two slots in the result must both be System messages.
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

        // No non-system message may appear before the last system message.
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
