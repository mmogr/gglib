//! Common test infrastructure for `gglib-agent` tests.
//!
//! - [`mock_tools`] — configurable [`ToolExecutorPort`] with per-tool
//!   behaviour (instant, delayed, fail, infra-error) and call recording.
//! - [`mock_llm`] — scripted [`LlmPort`] that returns pre-queued responses
//!   and the [`collect_events`] helper for draining an agent run into a
//!   `Vec<AgentEvent>`.
//! - [`event_assertions`] — shared predicates for asserting on
//!   [`AgentEvent`] slices (e.g. `has_final_answer`, `has_tool_start`).

pub mod event_assertions;
pub mod mock_llm;
pub mod mock_tools;
