//! `gglib model capabilities` handler.
//!
//! Displays or overrides the [`ModelCapabilities`] flags stored for a model.
//! All mutations go through [`ModelOps::set_capabilities`] in
//! `gglib-app-services`, which is the single shared implementation consumed
//! by this CLI, the Axum WebUI, and the Tauri app.
//!
//! [`ModelCapabilities`]: gglib_core::ModelCapabilities

use anyhow::{Result, anyhow};
use gglib_app_services::types::SetCapabilitiesRequest;
use gglib_app_services::{ModelDeps, ModelOps};
use gglib_core::ModelCapabilities;

use crate::bootstrap::CliContext;
use super::resolver;

/// Execute `gglib model capabilities <id> [--set FLAG]... [--unset FLAG]...`.
///
/// Without `--set` or `--unset` flags the command is read-only and prints the
/// current capability state.  With flags it applies the requested overrides
/// via [`ModelOps::set_capabilities`] and prints the updated state.
pub async fn execute(
    ctx: &CliContext,
    identifier: &str,
    set: Vec<String>,
    unset: Vec<String>,
) -> Result<()> {
    let core_model = resolver::resolve_model_identifier(ctx, identifier).await?;

    // Build-once — ModelOps is cheap and constructed the same way as in Axum/Tauri.
    let ops = ModelOps::new(ModelDeps {
        core: ctx.app.clone(),
        runner: ctx.runner.clone(),
        gguf_parser: ctx.gguf_parser.clone(),
    });

    // Read-only: no flags provided.
    if set.is_empty() && unset.is_empty() {
        let model = ops.get(core_model.id).await?;
        print_capabilities(core_model.id, &model.name, model.capabilities);
        return Ok(());
    }

    // Build the override request.
    let mut req = SetCapabilitiesRequest::default();

    for flag in &set {
        apply_flag(&mut req, flag, true)?;
    }
    for flag in &unset {
        apply_flag(&mut req, flag, false)?;
    }

    let gui_model = ops.set_capabilities(core_model.id, req).await?;

    println!(
        "Updated capabilities for model {} ({}):",
        core_model.id, gui_model.name
    );
    print_capabilities(core_model.id, &gui_model.name, gui_model.capabilities);

    Ok(())
}

/// Parse a capability flag name and set/clear the corresponding field.
fn apply_flag(req: &mut SetCapabilitiesRequest, flag: &str, value: bool) -> Result<()> {
    match flag {
        "supports-system-role" => req.supports_system_role = Some(value),
        "requires-strict-turns" => req.requires_strict_turns = Some(value),
        "supports-tool-calls" => req.supports_tool_calls = Some(value),
        "supports-reasoning" => req.supports_reasoning = Some(value),
        other => {
            return Err(anyhow!(
                "Unknown capability flag '{other}'.\n\
                 Valid flags: supports-system-role, requires-strict-turns, \
                 supports-tool-calls, supports-reasoning"
            ));
        }
    }
    Ok(())
}

/// Pretty-print the capability state for a model.
fn print_capabilities(id: i64, name: &str, caps: ModelCapabilities) {
    println!("Capabilities for model {id} ({name}):");
    println!(
        "  supports-system-role  : {}",
        flag_str(caps.contains(ModelCapabilities::SUPPORTS_SYSTEM_ROLE))
    );
    println!(
        "  requires-strict-turns : {}",
        flag_str(caps.contains(ModelCapabilities::REQUIRES_STRICT_TURNS))
    );
    println!(
        "  supports-tool-calls   : {}",
        flag_str(caps.contains(ModelCapabilities::SUPPORTS_TOOL_CALLS))
    );
    println!(
        "  supports-reasoning    : {}",
        flag_str(caps.contains(ModelCapabilities::SUPPORTS_REASONING))
    );
    if caps.is_empty() {
        println!("  (all flags unset — pass-through mode)");
    }
}

fn flag_str(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}
