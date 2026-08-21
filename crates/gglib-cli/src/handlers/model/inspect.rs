//! Inspect command handler.
//!
//! Displays full details for a single model — every stored field including
//! raw GGUF metadata, MoE topology, HuggingFace provenance, capability flags,
//! inference defaults, and timestamps.
//!
//! This handler is intentionally thin:
//! - Flexible identifier resolution via [`resolver::resolve_model_identifier`] (name **or** ID)
//! - Serving-status-aware DTO via `ModelOps::get_detail()` (same path as the Axum route)
//! - `--json` → serialize `ModelDetailDto` to stdout
//! - human mode → delegate to [`inspect_display::print_model_detail`]
//!
//! All terminal rendering lives in `presentation/inspect_display.rs`.

use anyhow::Result;

use super::resolver;
use crate::bootstrap::CliContext;
use crate::presentation::inspect_display;

/// Execute `gglib model inspect <identifier> [--metadata] [--json]`.
pub(crate) async fn execute(
    ctx: &CliContext,
    identifier: &str,
    show_metadata: bool,
    json: bool,
) -> Result<()> {
    // Step 1: resolve name-or-id through the one door.
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;

    // Step 2: fetch the full DTO via ModelOps so serving status is included.
    // This mirrors exactly what the Axum detail route does, ensuring CLI and
    // REST API output are consistent for a model that is currently being served.
    //
    // `NoopModelRuntime` rather than `ctx.runner`: this is a one-shot CLI
    // command with no shared `ProcessManager` to check, and a runner scoped
    // to this single invocation would never have anything running in it
    // regardless — this makes that explicit instead of asking a real runner
    // a question it can only ever answer "no" to.
    let ops = super::one_shot_model_ops(ctx);
    let dto = ops.get_detail(model.id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&dto)?);
        return Ok(());
    }

    inspect_display::print_model_detail(&dto, show_metadata);
    Ok(())
}
