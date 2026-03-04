//! Common test infrastructure for `gglib-agent` tests.
#![allow(dead_code)]
//!
//! - [`mock_tools`] — configurable [`ToolExecutorPort`] with per-tool
//!   behaviour (instant, delayed, fail, infra-error) and call recording.
//! - [`mock_llm`] — scripted [`LlmPort`] that returns pre-queued responses.
//! - [`event_assertions`] — shared predicates and helpers for asserting on
//!   [`AgentEvent`] slices, including [`event_assertions::collect_events`],
//!   `has_final_answer`, `has_tool_start`, etc.
//! - [`for_test`] — construct a customised [`AgentConfig`] from
//!   [`AgentConfig::default`] without boilerplate mutation blocks.

pub mod event_assertions;
pub mod mock_llm;
pub mod mock_tools;

use gglib_core::domain::agent::AgentConfig;

/// Build an [`AgentConfig`] starting from [`AgentConfig::default`] and
/// applying `f` to override individual fields.
///
/// Avoids repetitive `let mut c = AgentConfig::default(); …; c` blocks in
/// every test.
///
/// # Example
///
/// ```ignore
/// let config = common::for_test(|c| {
///     c.max_iterations = 3;
///     c.max_stagnation_steps = Some(2);
/// });
/// ```
pub fn for_test(f: impl FnOnce(&mut AgentConfig)) -> AgentConfig {
    let mut c = AgentConfig::default();
    f(&mut c);
    c
}
