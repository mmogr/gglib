//! `gglib council rewind <run-id> --wave N` — rewind a run to a previous
//! wave and re-execute from that point.
//!
//! Full implementation arrives in Phase 5. This stub is present so the
//! subcommand appears in `--help` and returns a clear error when called.

use anyhow::{Result, anyhow};

use crate::bootstrap::CliContext;

/// Phase 5 stub — returns an error explaining the command is not yet available.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    _ctx: &CliContext,
    _run_id: &str,
    _wave: u32,
    _note: Option<&str>,
    _port: Option<u16>,
    _model: Option<String>,
    _ctx_size: Option<String>,
) -> Result<()> {
    Err(anyhow!(
        "`council rewind` is not yet implemented (arriving in Phase 5)"
    ))
}
