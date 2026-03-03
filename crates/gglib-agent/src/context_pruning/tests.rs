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
                .is_some_and(|calls| calls.iter().any(|c| c.id == "call_0"))
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
            "non-System message at position {i} precedes all System messages; got {msg:?}"
        );
    }
}
