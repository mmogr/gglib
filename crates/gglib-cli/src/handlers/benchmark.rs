//! `gglib benchmark` — placeholder until full implementation in Step 3b.
use anyhow::Result;
use crate::benchmark_commands::BenchmarkCommand;
use crate::bootstrap::CliContext;

pub async fn dispatch(_ctx: &CliContext, _cmd: BenchmarkCommand) -> Result<()> {
    anyhow::bail!("benchmark command not yet implemented")
}
