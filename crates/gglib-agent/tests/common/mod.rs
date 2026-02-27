//! Common test infrastructure for `gglib-agent` integration tests.
//!
//! - [`mock_llm`] — configurable [`LlmCompletionPort`] that serves scripted
//!   responses without any HTTP server.
//! - [`mock_tools`] — configurable [`ToolExecutorPort`] with per-tool
//!   behaviour (instant, delayed, fail, infra-error) and call recording.

pub mod mock_llm;
pub mod mock_tools;
