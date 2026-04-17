//! Shared council suggestion orchestration.
//!
//! Runs a single-iteration `AgentLoop` with the council designer prompt,
//! parses the LLM's JSON output, and returns a [`SuggestedCouncil`].
//! Used by both the CLI and Axum consumers.

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};

use crate::AgentLoop;
use crate::council::config::SuggestedCouncil;
use crate::council::prompts::COUNCIL_DESIGNER_PROMPT;

use std::sync::Arc;

use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};

// ─── Public API ──────────────────────────────────────────────────────────────

/// Ask the LLM to design a council for the given topic.
///
/// Runs a single `AgentLoop` iteration with the designer prompt, extracts
/// the JSON payload from the response, and backfills default ids/colours.
///
/// When `refinement_history` is `Some`, the caller-provided messages are
/// appended after the system prompt instead of the default `User(topic)`.
/// This enables multi-turn refinement: the caller constructs a thread like
/// `[User(topic), Assistant(prior_suggestion), User(feedback)]`.
pub async fn suggest_council(
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    topic: &str,
    agent_count: u32,
    refinement_history: Option<Vec<AgentMessage>>,
) -> Result<SuggestedCouncil> {
    #[allow(clippy::literal_string_with_formatting_args)]
    let system = COUNCIL_DESIGNER_PROMPT
        .replace("{agent_count}", &agent_count.to_string())
        .replace("{user_topic}", topic);

    let messages = build_suggest_messages(&system, refinement_history, topic);

    let mut config = AgentConfig::default();
    config.max_iterations = 1;

    let agent = AgentLoop::build(llm, tool_executor, None);
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let handle = tokio::spawn(async move { agent.run(messages, config, tx).await });

    let mut content = String::new();
    let mut error_msg: Option<String> = None;
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::FinalAnswer { content: answer } => content = answer,
            AgentEvent::Error { message } => error_msg = Some(message),
            _ => {}
        }
    }
    let _ = handle.await;

    if let Some(msg) = error_msg {
        return Err(anyhow!("council suggestion failed: {msg}"));
    }
    if content.is_empty() {
        return Err(anyhow!("LLM did not return a council suggestion"));
    }

    let mut council = parse_suggested_council(&content)?;
    council.backfill_defaults();
    Ok(council)
}

// ─── Message construction ────────────────────────────────────────────────────

/// Build the message list for a suggest call.
///
/// Fresh suggest: `[System(prompt), User(topic)]`.
/// Refinement:    `[System(prompt)] + history` (caller-provided thread).
fn build_suggest_messages(
    system: &str,
    refinement_history: Option<Vec<AgentMessage>>,
    topic: &str,
) -> Vec<AgentMessage> {
    refinement_history.map_or_else(
        || {
            vec![
                AgentMessage::System {
                    content: system.to_owned(),
                },
                AgentMessage::User {
                    content: topic.to_owned(),
                },
            ]
        },
        |history| {
            let mut msgs = vec![AgentMessage::System {
                content: system.to_owned(),
            }];
            msgs.extend(history);
            msgs
        },
    )
}

// ─── Parsing helpers ─────────────────────────────────────────────────────────

/// Parse the LLM's response text as a [`SuggestedCouncil`].
///
/// Handles optional markdown JSON fences that small models often emit.
fn parse_suggested_council(raw: &str) -> Result<SuggestedCouncil> {
    let trimmed = strip_markdown_json(raw);
    serde_json::from_str(trimmed)
        .map_err(|e| anyhow!("failed to parse council suggestion: {e}\n\nRaw:\n{raw}"))
}

/// Strip optional ` ```json ... ``` ` fences from LLM output.
fn strip_markdown_json(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::agent::AssistantContent;

    #[test]
    fn build_messages_fresh_suggest() {
        // When refinement_history is None, messages should be
        // [System(prompt), User(topic)].
        let system = COUNCIL_DESIGNER_PROMPT
            .replace("{agent_count}", "3")
            .replace("{user_topic}", "test topic");

        let messages = build_suggest_messages(&system, None, "test topic");
        assert_eq!(messages.len(), 2);
        assert!(matches!(&messages[0], AgentMessage::System { .. }));
        assert!(matches!(&messages[1], AgentMessage::User { content } if content == "test topic"));
    }

    #[test]
    fn build_messages_with_refinement() {
        let system = COUNCIL_DESIGNER_PROMPT
            .replace("{agent_count}", "3")
            .replace("{user_topic}", "test topic");

        let history = vec![
            AgentMessage::User {
                content: "test topic".into(),
            },
            AgentMessage::Assistant {
                content: AssistantContent {
                    text: Some("{\"agents\": []}".into()),
                    tool_calls: vec![],
                },
            },
            AgentMessage::User {
                content: "add a security expert".into(),
            },
        ];

        let messages = build_suggest_messages(&system, Some(history), "test topic");
        assert_eq!(messages.len(), 4); // System + 3 history
        assert!(matches!(&messages[0], AgentMessage::System { .. }));
        assert!(matches!(&messages[1], AgentMessage::User { content } if content == "test topic"));
        assert!(matches!(&messages[2], AgentMessage::Assistant { .. }));
        assert!(
            matches!(&messages[3], AgentMessage::User { content } if content == "add a security expert")
        );
    }

    #[test]
    fn strip_plain_json() {
        let input = r#"{"agents": []}"#;
        assert_eq!(strip_markdown_json(input), input);
    }

    #[test]
    fn strip_fenced_json() {
        let input = "```json\n{\"agents\": []}\n```";
        assert_eq!(strip_markdown_json(input), r#"{"agents": []}"#);
    }

    #[test]
    fn strip_bare_fences() {
        let input = "```\n{\"agents\": []}\n```";
        assert_eq!(strip_markdown_json(input), r#"{"agents": []}"#);
    }
}
