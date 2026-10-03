//! Inspect command handler.
//!
//! Displays full details for a single model — every stored field including
//! raw GGUF metadata, `MoE` topology, `HuggingFace` provenance, capability flags,
//! inference defaults, and timestamps.
//!
//! This handler is intentionally thin:
//! - Flexible identifier resolution via [`resolver::resolve_for`] (name **or** ID)
//! - Serving-status-aware DTO via `ModelOps::get_detail()` (same path as the Axum route)
//! - With `--remote`, the paired machine's own answer for the identifier,
//!   read through the daemon, with no file path: that machine's layout means
//!   nothing here
//! - `--json` → serialize `ModelDetailDto` to stdout
//! - human mode → delegate to [`inspect_display::print_model_detail`]
//!
//! All terminal rendering lives in `presentation/inspect_display.rs`.

use anyhow::{Context as _, Result};
use gglib_core::domain::{ModelAction, ModelDetailDto};

use super::resolver;
use crate::bootstrap::CliContext;
use crate::presentation::inspect_display;
use crate::target::{Target, far_wire};

/// Execute `gglib model inspect <identifier> [--metadata] [--json]`, on
/// this machine or, with `--remote`, the paired one.
pub(crate) async fn execute(
    ctx: &CliContext,
    target: Target,
    identifier: &str,
    show_metadata: bool,
    json: bool,
) -> Result<()> {
    let print = |dto: &ModelDetailDto| -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(dto)?);
        } else {
            inspect_display::print_model_detail(dto, show_metadata);
        }
        Ok(())
    };
    target
        .run(
            async || {
                // Resolve name-or-id through the one door, then fetch the
                // full DTO via ModelOps so serving status is included,
                // exactly as the Axum detail route does.
                let model = resolver::resolve_for(ctx, identifier, ModelAction::Detail).await?;
                print(&super::one_shot_model_ops(ctx).get_detail(model.id).await?)
            },
            async || {
                let paired = target.paired(ctx).await?;
                let found = (paired.handle.paired_model(identifier).await)
                    .with_context(|| format!("looking up '{identifier}' on {}", paired.name))?;
                print(&found.detail)?;
                if !json {
                    let wire = far_wire(found.detail.id, found.profile.as_deref());
                    println!(
                        "\nOn {}. Chat with it: gglib chat {wire} --remote",
                        paired.name
                    );
                }
                Ok(())
            },
        )
        .await
}
