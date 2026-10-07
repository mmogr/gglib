//! `gglib model capabilities` handler.
//!
//! Displays or overrides the [`ModelCapabilities`] flags stored for a model.
//! All mutations go through [`ModelOps::set_capabilities`] in
//! `gglib-app-services`, which is the single shared implementation consumed
//! by this CLI, the Axum `WebUI`, and the Tauri app.
//!
//! [`ModelCapabilities`]: gglib_core::ModelCapabilities
//! [`ModelOps::set_capabilities`]: gglib_app_services::ModelOps::set_capabilities

use anyhow::{Result, anyhow};
use gglib_app_services::types::SetCapabilitiesRequest;
use gglib_core::ModelCapabilities;

use super::resolver;
use crate::bootstrap::CliContext;
use crate::presentation::capability_flags::{CAPABILITY_FLAGS, capability_lines};

/// Execute `gglib model capabilities <id> [--set FLAG]... [--unset FLAG]...`.
///
/// Without `--set` or `--unset` flags the command is read-only and prints the
/// current capability state.  With flags it applies the requested overrides
/// via [`ModelOps::set_capabilities`] and prints the updated state.
///
/// [`ModelOps::set_capabilities`]: gglib_app_services::ModelOps::set_capabilities
pub(crate) async fn execute(
    ctx: &CliContext,
    identifier: &str,
    set: Vec<String>,
    unset: Vec<String>,
) -> Result<()> {
    let core_model = resolver::resolve_model_identifier(ctx, identifier).await?;

    // Build-once — ModelOps is cheap and constructed the same way as in Axum/Tauri.
    //
    // `NoopModelRuntime` rather than `ctx.runner`: a one-shot CLI command has
    // no shared `ProcessManager` to check, and this handler never touches
    // serving status anyway (`get`/`set_capabilities` only).
    let ops = super::one_shot_model_ops(ctx);

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

/// Set or clear the field of `req` that the flag called `name` is
/// overridden by. Clap takes `--set` and `--unset` values from
/// [`CAPABILITY_FLAGS`], so a name that is not in it does not reach here.
fn apply_flag(req: &mut SetCapabilitiesRequest, name: &str, value: bool) -> Result<()> {
    let mut flags = CAPABILITY_FLAGS.iter();
    let flag = flags.find(|flag| flag.name == name);
    let flag = flag.ok_or_else(|| anyhow!("Unknown capability flag '{name}'."))?;
    *(flag.field)(req) = Some(value);
    Ok(())
}

/// Pretty-print the capability state for a model.
fn print_capabilities(id: i64, name: &str, caps: ModelCapabilities) {
    println!("Capabilities for model {id} ({name}):");
    for line in capability_lines(caps) {
        println!("{line}");
    }
    if caps.is_empty() {
        println!("  (all flags unset — pass-through mode)");
    }
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
