//! The flag `chat` and `question` share for reusing a llama-server that is
//! already running here.

use clap::Args;

/// A llama-server already running on this machine.
///
/// Nothing set asks the daemon to start the model here; `--port` reuses
/// one. Which *machine* a turn runs on is no longer this struct's question:
/// that is the global `--remote`, declared once on the root parser and
/// carried as a [`Target`](crate::target::Target) (ADR 0013). The two still
/// exclude each other, because a port here and a machine there name
/// different places.
#[derive(Args, Debug, Clone, Default)]
pub struct UpstreamArgs {
    /// Reuse an already-running llama-server on this port (skips auto-start)
    #[arg(long, conflicts_with = "remote")]
    pub port: Option<u16>,
}
