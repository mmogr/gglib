//! Port definition for streaming LLM chat completions.
//!
//! This module defines the infrastructure contract that the agent loop uses to
//! drive an LLM.  The port is intentionally narrow: it speaks **domain types**
//! ([`AgentMessage`], [`ToolDefinition`], [`LlmStreamEvent`]) and hides all
//! vendor wire-format details (`OpenAI` JSON schemas, SSE framing, HTTP headers,
//! etc.) behind the trait boundary.
//!
//! # Adapter responsibility
//!
//! A concrete implementation (e.g. in `gglib-axum` or `gglib-proxy`) is
//! responsible for:
//!
//! 1. Translating `&[AgentMessage]` into the vendor's `messages` array,
//!    serialising `ToolCall::arguments` (`serde_json::Value`) into the JSON
//!    string form that OpenAI-compatible APIs require.
//! 2. Translating `&[ToolDefinition]` into the vendor's `tools` array.
//! 3. Parsing the streaming SSE response into a sequence of [`LlmStreamEvent`]
//!    values, accumulating incremental tool-call deltas where necessary.
//! 4. When `response_format` is `Some`, injecting the appropriate
//!    `response_format` / `grammar` field into the vendor request body.
//!
//! The agent loop never sees HTTP, never sees `reqwest`, and never contains a
//! single OpenAI-specific field name.

use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use futures_core::Stream;

use crate::domain::agent::{AgentMessage, LlmStreamEvent, ToolDefinition};

/// The endpoint refused the request with a code of its own, and asking
/// again would be refused the same way.
///
/// Returned by [`LlmCompletionPort::chat_stream`] inside its `anyhow::Error`,
/// so the agent loop can carry `code` to the run's end rather than leave it
/// in the sentence. Its text is `reason` alone.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct UpstreamRefusal {
    /// The HTTP status the endpoint answered with.
    pub status: u16,
    /// The `error.code` the endpoint's body wrote, as it wrote it: gglib's
    /// own, or one relayed from llama-server or the tunnel.
    pub code: String,
    /// What a person reads: the endpoint's status, type, code and message.
    pub reason: String,
}

// =============================================================================
// LlmCompletionPort
// =============================================================================

/// Port that the agent loop uses to drive a streaming LLM.
///
/// Implementations translate domain messages + tool definitions into
/// vendor-specific HTTP requests and stream back [`LlmStreamEvent`] values.
///
/// # Contract
///
/// - The returned stream **must** end with exactly one [`LlmStreamEvent::Done`]
///   item, even when the finish reason is abnormal (e.g. `"length"`).
/// - Text and tool-call delta events may interleave freely before `Done`.
/// - An `Err` item in the stream signals an unrecoverable infrastructure error;
///   the agent loop will surface it as [`super::agent::AgentError::Internal`].
/// - An `Err` from starting the request may be an [`UpstreamRefusal`], which
///   the agent loop surfaces as [`super::agent::AgentError::Refused`].
#[async_trait]
pub trait LlmCompletionPort: Send + Sync {
    /// Begin a chat-completion request and return a live event stream.
    ///
    /// # Parameters
    ///
    /// - `messages` — conversation history in domain form.
    /// - `tools` — tool schemas to advertise to the model.
    ///
    /// # Returns
    ///
    /// A pinned, heap-allocated, `Send`-able stream of [`LlmStreamEvent`].
    /// The caller drives the stream by polling it; each item is either a
    /// successfully parsed event or an infrastructure error.
    async fn chat_stream(
        &self,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>>;

    /// As [`chat_stream`](Self::chat_stream), for a reply that must call one
    /// of `tools`: the request demands a call instead of leaving it to the
    /// model (`OpenAI`'s `tool_choice: "required"`).
    ///
    /// The loop asks this of a run's first reply only, with the one tool
    /// its caller demanded
    /// ([`FirstCall`](crate::domain::agent::FirstCall)). The default sends
    /// the request as `chat_stream` does, for an implementation with no way
    /// to demand; the loop fails the run when no call comes back, so the
    /// demand holds either way.
    async fn chat_stream_requiring_call(
        &self,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        self.chat_stream(messages, tools).await
    }
}
