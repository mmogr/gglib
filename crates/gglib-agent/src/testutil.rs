//! Shared test utilities for `gglib-agent` unit tests.
//!
//! Provides lightweight mocks that are reused across multiple `#[cfg(test)]`
//! modules within the crate.  Integration tests under `tests/` have their own
//! richer mocks in `tests/common/`; this module serves unit tests that live
//! inside `src/`.

use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream;
use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AgentMessage, LlmStreamEvent, ToolCall, ToolDefinition, ToolResult};
use tokio::sync::Mutex;

// =============================================================================
// ScriptedLlm — pre-scripted LLM response queue
// =============================================================================

/// A [`LlmCompletionPort`] that pops pre-built [`LlmStreamEvent`] sequences
/// in FIFO order on each `chat_stream` call.
///
/// If `chat_stream` is called after the queue is exhausted, it returns an
/// `Err` — which the agent loop surfaces as `AgentError::Internal`.
pub struct ScriptedLlm {
    responses: Mutex<std::collections::VecDeque<Vec<LlmStreamEvent>>>,
}

impl ScriptedLlm {
    pub fn new(responses: Vec<Vec<LlmStreamEvent>>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
        }
    }
}

#[async_trait]
impl LlmCompletionPort for ScriptedLlm {
    async fn chat_stream(
        &self,
        _messages: &[AgentMessage],
        _tools: &[ToolDefinition],
    ) -> Result<Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        let events = self
            .responses
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("ScriptedLlm has no more responses"))?;
        Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
    }
}

// =============================================================================
// FailingLlm — always returns Err from chat_stream
// =============================================================================

/// A [`LlmCompletionPort`] that always fails `chat_stream` with a fixed error.
pub struct FailingLlm;

#[async_trait]
impl LlmCompletionPort for FailingLlm {
    async fn chat_stream(
        &self,
        _messages: &[AgentMessage],
        _tools: &[ToolDefinition],
    ) -> Result<Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        Err(anyhow::anyhow!("simulated LLM connection failure"))
    }
}

// =============================================================================
// StubExecutor — always returns a successful result; tool list is configurable
// =============================================================================

/// A [`ToolExecutorPort`] stub that returns `success = true` for every call.
///
/// The advertised tool list is controlled at construction time, covering both
/// "one-tool" and "no-tools" scenarios without duplicating an implementation.
/// Replaces the previous `OkExecutor` (hard-coded to `"do_thing"`) and
/// `NoToolExecutor` (listed no tools) pair.
pub struct StubExecutor {
    tools: Vec<ToolDefinition>,
}

impl StubExecutor {
    /// Create a stub that advertises tools with the given names.
    /// Pass `&[]` when the tool list is irrelevant to the test.
    pub fn with_tools(names: &[&str]) -> Self {
        Self {
            tools: names.iter().map(|n| ToolDefinition::new(*n)).collect(),
        }
    }
}

#[async_trait]
impl ToolExecutorPort for StubExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.clone()
    }
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            content: "ok".into(),
            success: true,
            wait_ms: 0,
            duration_ms: 0,
        })
    }
}

// =============================================================================
// DelayedExecutor — sleeps before returning a successful result
// =============================================================================

/// A [`ToolExecutorPort`] that sleeps for `delay_ms` milliseconds before
/// returning a successful result.  Useful for exercising per-tool timeouts.
pub struct DelayedExecutor {
    pub delay_ms: u64,
}

#[async_trait]
impl ToolExecutorPort for DelayedExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            content: "slow ok".into(),
            success: true,
            wait_ms: 0,
            duration_ms: self.delay_ms,
        })
    }
}

// =============================================================================
// Event sequence helpers
// =============================================================================

/// Build a `[ToolCallDelta, Done]` event sequence for a single tool call.
pub fn tool_call_events(id: &str, name: &str) -> Vec<LlmStreamEvent> {
    vec![
        LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some(id.into()),
            name: Some(name.into()),
            arguments: Some("{}".into()),
        },
        LlmStreamEvent::Done {
            finish_reason: "tool_calls".into(),
        },
    ]
}

/// Build a `[TextDelta, Done]` event sequence.
pub fn text_events(text: &str) -> Vec<LlmStreamEvent> {
    vec![
        LlmStreamEvent::TextDelta {
            content: text.into(),
        },
        LlmStreamEvent::Done {
            finish_reason: "stop".into(),
        },
    ]
}

/// Build a `[TextDelta, ToolCallDelta, Done]` event sequence.
pub fn text_and_tool_events(text: &str, id: &str, name: &str) -> Vec<LlmStreamEvent> {
    vec![
        LlmStreamEvent::TextDelta {
            content: text.into(),
        },
        LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some(id.into()),
            name: Some(name.into()),
            arguments: Some("{}".into()),
        },
        LlmStreamEvent::Done {
            finish_reason: "tool_calls".into(),
        },
    ]
}
