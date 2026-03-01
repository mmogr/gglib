//! Mock implementation of [`LlmCompletionPort`] for integration testing.
//!
//! Serves pre-scripted sequences of [`LlmStreamEvent`] values, one response
//! per `chat_stream` call.  Build the response queue using the builder API:
//!
//! ```rust,ignore
//! let llm = MockLlmPort::new()
//!     .push(MockLlmResponse::tool_call("tc1", "search", serde_json::json!({})))
//!     .push(MockLlmResponse::text("Here are the results."));
//! ```

use std::collections::VecDeque;
use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream;
use gglib_core::domain::agent::{
    AgentEvent, AgentMessage, LlmStreamEvent, ToolCall, ToolDefinition,
};
use gglib_core::ports::LlmCompletionPort;
use tokio::sync::{Mutex, mpsc};

// =============================================================================
// MockLlmResponse — one scripted turn
// =============================================================================

/// A single scripted response that [`MockLlmPort`] will emit for one
/// `chat_stream` call.
///
/// Converts into the sequence of [`LlmStreamEvent`] values:
/// `TextDelta?` → `ToolCallDelta*` → `Done`.
pub struct MockLlmResponse {
    /// Optional assistant text (emitted as one [`LlmStreamEvent::TextDelta`]).
    pub content: Option<String>,
    /// Tool invocations the model "requests".
    pub tool_calls: Vec<ToolCall>,
    /// The `OpenAI` finish reason (`"stop"`, `"tool_calls"`, etc.).
    pub finish_reason: String,
}

impl MockLlmResponse {
    /// A plain-text response with `finish_reason = "stop"`.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            tool_calls: vec![],
            finish_reason: "stop".into(),
        }
    }

    /// A single-tool-call response with `finish_reason = "tool_calls"`.
    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self {
            content: None,
            tool_calls: vec![ToolCall {
                id: id.into(),
                name: name.into(),
                arguments: args,
            }],
            finish_reason: "tool_calls".into(),
        }
    }

    /// Expand into the raw [`LlmStreamEvent`] sequence the adapter would emit.
    fn into_events(self) -> Vec<LlmStreamEvent> {
        let mut events = Vec::new();
        if let Some(text) = self.content {
            events.push(LlmStreamEvent::TextDelta { content: text });
        }
        for (index, call) in self.tool_calls.into_iter().enumerate() {
            // Emit one delta per tool call — id and name in the first delta,
            // full arguments in the same delta (single chunk, as compatible
            // with the stream_collector's accumulation logic).
            events.push(LlmStreamEvent::ToolCallDelta {
                index,
                id: Some(call.id),
                name: Some(call.name),
                arguments: Some(call.arguments.to_string()),
            });
        }
        events.push(LlmStreamEvent::Done {
            finish_reason: self.finish_reason,
        });
        events
    }
}

// =============================================================================
// MockLlmPort
// =============================================================================

/// Mock [`LlmCompletionPort`] that returns pre-scripted responses in FIFO order.
///
/// If `chat_stream` is called after the queue is exhausted, it returns an
/// `Err` — which the agent loop surfaces as `AgentError::Internal`.  In
/// practice, tests should push at least as many responses as the expected
/// number of LLM calls.
///
/// Every `chat_stream` call appends a snapshot of the `messages` argument to
/// an internal log accessible via [`MockLlmPort::messages_received`], letting
/// tests assert on what the agent actually passed to the LLM (e.g. to verify
/// that context pruning reduced the message count).
pub struct MockLlmPort {
    responses: Mutex<VecDeque<Vec<LlmStreamEvent>>>,
    /// Snapshots of the `messages` slice passed to each `chat_stream` call,
    /// in call order.
    messages_received: Mutex<Vec<Vec<AgentMessage>>>,
}

impl MockLlmPort {
    /// Create an empty mock with no scripted responses.
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(VecDeque::new()),
            messages_received: Mutex::new(Vec::new()),
        }
    }

    /// Return a snapshot of all `messages` arguments passed to `chat_stream`,
    /// in call order.
    ///
    /// Useful for asserting that context pruning reduced the number of messages
    /// the agent actually sent to the LLM.
    pub async fn messages_received(&self) -> Vec<Vec<AgentMessage>> {
        self.messages_received.lock().await.clone()
    }

    /// Append one scripted response to the queue (builder-style, takes `self`).
    ///
    /// # Panics
    ///
    /// Panics if called concurrently — this is a build-time helper.
    pub fn push(self, response: MockLlmResponse) -> Self {
        self.responses
            .try_lock()
            .expect("MockLlmPort::push called concurrently")
            .push_back(response.into_events());
        self
    }

    /// Append many responses from an iterator (builder-style).
    ///
    /// # Panics
    ///
    /// Panics if called concurrently — this is a build-time helper.
    pub fn push_many(self, responses: impl IntoIterator<Item = MockLlmResponse>) -> Self {
        let mut guard = self
            .responses
            .try_lock()
            .expect("MockLlmPort::push_many called concurrently");
        for r in responses {
            guard.push_back(r.into_events());
        }
        drop(guard);
        self
    }
}

impl Default for MockLlmPort {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmCompletionPort for MockLlmPort {
    async fn chat_stream(
        &self,
        messages: &[AgentMessage],
        _tools: &[ToolDefinition],
    ) -> Result<Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        // Record a snapshot of the messages for test inspection.
        self.messages_received
            .lock()
            .await
            .push(messages.to_vec());

        let events = self
            .responses
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("MockLlmPort: no more scripted responses"))?;
        Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
    }
}

// =============================================================================
// collect_events — test helper
// =============================================================================

/// Drain all events buffered in `rx` after the sending end has been dropped.
///
/// [`AgentLoopPort::run`] takes the [`mpsc::Sender`] by value and drops it on
/// return, so by the time a test calls this helper the channel is already
/// closed — `recv()` will return `None` after the last buffered event.
pub async fn collect_events(mut rx: mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    events
}
