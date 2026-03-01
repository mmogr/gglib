//! Common test infrastructure for `gglib-agent` tests.
//!
//! - [`mock_tools`] — configurable [`ToolExecutorPort`] with per-tool
//!   behaviour (instant, delayed, fail, infra-error) and call recording.
//! - [`mock_llm`] — scripted [`LlmPort`] that returns pre-queued responses.
//! - [`event_assertions`] — shared predicates and helpers for asserting on
//!   [`AgentEvent`] slices, including [`event_assertions::collect_events`],
//!   `has_final_answer`, `has_tool_start`, etc.

pub mod event_assertions;
pub mod mock_llm;
pub mod mock_tools;
