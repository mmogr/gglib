//! Common test infrastructure for `gglib-agent` tests.
//!
//! - [`mock_tools`] — configurable [`ToolExecutorPort`] with per-tool
//!   behaviour (instant, delayed, fail, infra-error) and call recording.
//! - [`mock_llm`] — scripted [`LlmPort`] that returns pre-queued responses
//!   and the [`collect_events`] helper for draining an agent run into a
//!   `Vec<AgentEvent>`.

pub mod mock_llm;
pub mod mock_tools;
