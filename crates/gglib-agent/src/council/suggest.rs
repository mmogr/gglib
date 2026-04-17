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
pub async fn suggest_council(
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    topic: &str,
    agent_count: u32,
) -> Result<SuggestedCouncil> {
    #[allow(clippy::literal_string_with_formatting_args)]
    let system = COUNCIL_DESIGNER_PROMPT
        .replace("{agent_count}", &agent_count.to_string())
        .replace("{user_topic}", topic);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: topic.to_owned(),
        },
    ];

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
