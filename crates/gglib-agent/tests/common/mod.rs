//! Common test infrastructure for `gglib-agent` tests.
//!
//! - [`mock_tools`] — configurable [`ToolExecutorPort`] with per-tool
//!   behaviour (instant, delayed, fail, infra-error) and call recording.
//!
//! For LLM mocking and `collect_events`, include `mock_llm.rs` directly in
//! each test binary that needs it:
//!
//! ```rust,ignore
//! #[path = "common/mock_llm.rs"]
//! mod mock_llm;
//! ```

pub mod mock_tools;
