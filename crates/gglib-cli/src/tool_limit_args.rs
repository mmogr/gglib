//! The two flags `chat` and `question` share for bounding tool execution.

use clap::Args;

/// How long one tool may run, and how many run at once.
#[derive(Args, Debug, Clone, Copy, Default)]
pub struct ToolLimitArgs {
    /// Per-tool execution timeout in milliseconds
    #[arg(long = "tool-timeout-ms")]
    pub tool_timeout_ms: Option<u64>,
    /// Maximum number of tools executed in parallel per iteration
    #[arg(long = "max-parallel")]
    pub max_parallel: Option<usize>,
}
