//! `gglib config settings set` — write the fields a person named, and print them.
//!
//! Each flag is named once here, where it becomes a field of the
//! [`SettingsUpdate`]. The merge, the validation and the write are the
//! settings service's, in one step, so a refused value stores nothing and
//! there is no second merge here to keep in step with the real one. The keys
//! to print are read back off the update.

use std::collections::BTreeSet;

use anyhow::Result;

use gglib_core::SettingsUpdate;

use super::resolve_model_display;
use super::settings_display::{print_display_rows, settings_display_rows};
use crate::bootstrap::CliContext;
use crate::config_commands::SettingsSetArgs;

/// The update the flags a person passed amount to.
fn update_from(args: SettingsSetArgs) -> SettingsUpdate {
    SettingsUpdate {
        default_download_path: args.default_download_path.map(Some),
        default_context_size: args.default_context_size.map(Some),
        proxy_port: args.proxy_port.map(Some),
        llama_base_port: args.llama_base_port.map(Some),
        max_download_queue_size: args.max_download_queue_size.map(Some),
        show_memory_fit_indicators: args.show_memory_fit_indicators.map(Some),
        max_tool_iterations: args.max_tool_iterations.map(Some),
        max_stagnation_steps: args.max_stagnation_steps.map(Some),
        default_model_id: None,
        inference_defaults: None,
        inference_profiles: None,
        setup_completed: None,
        title_generation_prompt: None,
        bind_host: args.bind_host.map(Some),
        share_lan: args.share_lan.map(Some),
        proxy_api_key: args.proxy_api_key.map(Some),
        trust_client_sampling: args.trust_client_sampling.map(Some),
        loop_guard_mode: args.loop_guard_mode.map(|m| Some(m.into())),
        tool_call_repair: args.tool_call_repair.map(Some),
        agentic_sampling: args.agentic_sampling.map(Some),
        proxy_autostart: args.proxy_autostart.map(Some),
        close_to_tray: args.close_to_tray.map(Some),
        start_at_login: args.start_at_login.map(Some),
        // Written by `gglib remote join`, never by hand (ADR 0012).
        remote_pairing: None,
        // Written by `gglib remote enable`/`disable`, never by hand: setting
        // this by hand would say a machine is reachable with nothing bound.
        remote_enabled: None,
        remote_serve: None,
        // Written by inviting and forgetting devices, not by a settings flag.
        remote_devices: None,
    }
}

/// The kebab-case key of every field `update` writes.
///
/// A field the update leaves alone serialises as `null`. So would one it
/// clears, which no flag here can ask for.
fn changed_keys(update: &SettingsUpdate) -> Result<BTreeSet<String>> {
    let fields = serde_json::to_value(update)?;
    Ok(fields
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, _)| key.replace('_', "-"))
        .collect())
}

/// Apply the flags a person passed, then print only what changed.
pub(super) async fn handle_set(ctx: &CliContext, args: SettingsSetArgs) -> Result<()> {
    let update = update_from(args);
    let changed = changed_keys(&update)?;
    if changed.is_empty() {
        println!("No settings provided. Use --help to see available options.");
        return Ok(());
    }

    let updated = ctx.app.settings().update(update).await?;
    let model_display = resolve_model_display(ctx, &updated).await?;
    let all_rows = settings_display_rows(&updated, model_display);

    // Match exact key OR any dot-notation sub-row that starts with
    // "{changed_key}." — needed for nested fields such as inference-defaults.
    let changed_rows: Vec<_> = all_rows
        .into_iter()
        .filter(|(k, _)| {
            changed
                .iter()
                .any(|c| k == c || k.starts_with(&format!("{c}.")))
        })
        .collect();

    println!("✓ Settings updated successfully:");
    print_display_rows(&changed_rows);
    Ok(())
}

#[cfg(test)]
#[path = "set_tests.rs"]
mod tests;
