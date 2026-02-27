//! Maps [`ChatArgs`] flags to an [`AgentLoop`] composition root and handles
//! llama-server lifecycle (auto-start or port reuse).
//!
//! The full implementation is added in the next commit.

use anyhow::Result;

use gglib_agent::AgentLoop;
use gglib_core::ProcessHandle;

use crate::bootstrap::CliContext;
use crate::handlers::chat::ChatArgs;

/// Compose the [`AgentLoop`] for an agentic chat session.
///
/// Returns `(loop, server_handle, server_started)`.
/// When `server_started` is `true`, the caller must stop the server on exit.
pub async fn compose(
    _ctx: &CliContext,
    _args: &ChatArgs,
) -> Result<(AgentLoop, ProcessHandle, bool)> {
    todo!("agent_chat::config::compose — full implementation in Commit 4")
}
