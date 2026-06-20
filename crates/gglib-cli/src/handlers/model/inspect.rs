//! Inspect command handler.
//!
//! Displays full details for a single model — every stored field including
//! raw GGUF metadata, MoE topology, HuggingFace provenance, capability flags,
//! inference defaults, and timestamps.
//!
//! This handler is intentionally thin:
//! - Flexible identifier resolution via `AppCore::models().get()` (name **or** ID)
//! - Serving-status-aware DTO via `ModelOps::get_detail()` (same path as the Axum route)
//! - `--json` → serialize `ModelDetailDto` to stdout
//! - human mode → delegate to [`inspect_display::print_model_detail`]
//!
//! All terminal rendering lives in `presentation/inspect_display.rs`.

use anyhow::Result;
use gglib_app_services::{ModelDeps, ModelOps};

use crate::bootstrap::CliContext;
use crate::presentation::inspect_display;

/// Execute `gglib model inspect <identifier> [--metadata] [--json]`.
pub async fn execute(
    ctx: &CliContext,
    identifier: &str,
    show_metadata: bool,
    json: bool,
) -> Result<()> {
    // Step 1: resolve name-or-id via the flexible core service.
    let model = match ctx.app.models().get(identifier).await? {
        Some(m) => m,
        None => {
            eprintln!("No model found matching: '{identifier}'");
            eprintln!("Use 'gglib model list' to see available models.");
            return Ok(());
        }
    };

    // Step 2: fetch the full DTO via ModelOps so serving status is included.
    // This mirrors exactly what the Axum detail route does, ensuring CLI and
    // REST API output are consistent for a model that is currently being served.
    let ops = ModelOps::new(ModelDeps {
        core: ctx.app.clone(),
        runner: ctx.runner.clone(),
        gguf_parser: ctx.gguf_parser.clone(),
    });
    let dto = ops.get_detail(model.id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&dto)?);
        return Ok(());
    }

    inspect_display::print_model_detail(&dto, show_metadata);
    Ok(())
}


