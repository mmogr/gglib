//! Inspect command handler.
//!
//! Displays full details for a single model — every stored field including
//! raw GGUF metadata, MoE topology, HuggingFace provenance, capability flags,
//! inference defaults, and timestamps.

use anyhow::Result;

use crate::bootstrap::CliContext;

/// Execute the inspect command.
pub async fn execute(
    _ctx: &CliContext,
    _identifier: &str,
    _metadata: bool,
    _json: bool,
) -> Result<()> {
    todo!("Phase 3: implement inspect handler")
}
