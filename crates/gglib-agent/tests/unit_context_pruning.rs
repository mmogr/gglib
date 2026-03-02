//! Unit tests for [`gglib_agent::prune_for_budget`] and [`gglib_agent::total_chars`].
//!
//! Tests lived in `src/context_pruning.rs` until they were extracted here to
//! keep production source files small. The helper functions (`system`, `user`,
//! `assistant_text`, `assistant_with_calls`, `tool_result`) were also inlined
//! here so the test logic remains self-contained.

use gglib_agent::{prune_for_budget, total_chars};
use gglib_core::{AgentConfig, AgentMessage, AssistantContent, ToolCall};
use serde_json::json;

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
    //
    // Scenario: assistant calls [tc_old, tc_new]. tc_old's result is older
    // than prune_keep_tool_messages=1, so it gets dropped; tc_new's result
    // is the most recent and is retained.  The assistant message should
    // survive with only [tc_new] in its tool_calls.
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
    // Use a very tight budget and an explicit tail window of 2 so pass-2 is
    // forced and the outcome is deterministic.
    //
    // Character accounting (pass-2 survivors):
    //   system("S")                    →  1 char  (always kept)
    //   user("U-recent")               →  8 chars ─┐ last 2 non-system
    //   assistant_text("Best answer.") → 12 chars ─┘
    //   total = 21 ≤ 50  ✓
    let msgs = vec![
        system("S"),                        // 1 char  — always kept
        user("U1"),                         // 2 chars — outside tail, dropped
        assistant_text(&"A".repeat(5_000)), // 5 000 chars — forces pass-2
        user("U-recent"),                   // 8 chars ─┐ tail of 2
        assistant_text("Best answer."),     // 12 chars ─┘
    ];

    let mut cfg = AgentConfig::default();
    cfg.context_budget_chars = 50;
    cfg.prune_keep_tail_messages = 2;
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

    let result = prune_for_budget(msgs, &cfg);

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
