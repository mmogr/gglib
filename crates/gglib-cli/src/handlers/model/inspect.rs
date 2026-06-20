//! Inspect command handler.
//!
//! Displays full details for a single model — every stored field including
//! raw GGUF metadata, MoE topology, HuggingFace provenance, capability flags,
//! inference defaults, and timestamps.
//!
//! This handler is intentionally thin:
//! - Model lookup via `AppCore::models().get()` (handles name **or** ID)
//! - `--json` → serialize [`ModelDetailDto`] to stdout
//! - human mode → delegate to [`inspect_display::print_model_detail`]
//!
//! All terminal rendering lives in `presentation/inspect_display.rs`.

use anyhow::Result;
use gglib_app_services::types::ModelDetailDto;

use crate::bootstrap::CliContext;
use crate::presentation::inspect_display;

/// Execute `gglib model inspect <identifier> [--metadata] [--json]`.
pub async fn execute(
    ctx: &CliContext,
    identifier: &str,
    show_metadata: bool,
    json: bool,
) -> Result<()> {
    let model = match ctx.app.models().get(identifier).await? {
        Some(m) => m,
        None => {
            eprintln!("No model found matching: '{identifier}'");
            eprintln!("Use 'gglib model list' to see available models.");
            return Ok(());
        }
    };

    let dto = ModelDetailDto::from_model(model, false, None);

    if json {
        println!("{}", serde_json::to_string_pretty(&dto)?);
        return Ok(());
    }

    inspect_display::print_model_detail(&dto, show_metadata);
    Ok(())
}

