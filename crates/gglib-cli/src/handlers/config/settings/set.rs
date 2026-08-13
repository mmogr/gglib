//! `gglib config settings set` — write the fields a person named, and print them.
//!
//! Its own module because the flag list is long and repetitive by nature: each
//! field appears in the `changed` set (for the confirmation print), in the
//! `SettingsUpdate` (for the write), and in the prospective merge (so
//! validation rejects a bad value before anything is persisted). Three
//! mentions per setting is what makes this file grow with every knob, and what
//! made `tool_call_repair` easy to miss in one of the three for months.

use std::collections::BTreeSet;

use anyhow::Result;

use gglib_core::{SettingsUpdate, validate_settings};

use super::resolve_model_display;
use super::settings_display::{print_display_rows, settings_display_rows};
use crate::bootstrap::CliContext;
use crate::config_commands::SettingsSetArgs;

/// Apply the flags a person passed, then print only what changed.
pub(super) async fn handle_set(ctx: &CliContext, args: SettingsSetArgs) -> Result<()> {
    // Collect the kebab-case keys of every flag that was provided.
    let mut changed: BTreeSet<&str> = BTreeSet::new();
    if args.default_download_path.is_some() {
        changed.insert("default-download-path");
    }
    if args.default_context_size.is_some() {
        changed.insert("default-context-size");
    }
    if args.proxy_port.is_some() {
        changed.insert("proxy-port");
    }
    if args.llama_base_port.is_some() {
        changed.insert("llama-base-port");
    }
    if args.max_download_queue_size.is_some() {
        changed.insert("max-download-queue-size");
    }
    if args.max_tool_iterations.is_some() {
        changed.insert("max-tool-iterations");
    }
    if args.max_stagnation_steps.is_some() {
        changed.insert("max-stagnation-steps");
    }
    if args.show_memory_fit_indicators.is_some() {
        changed.insert("show-memory-fit-indicators");
    }
    if args.bind_host.is_some() {
        changed.insert("bind-host");
    }
    if args.share_lan.is_some() {
        changed.insert("share-lan");
    }
    if args.proxy_api_key.is_some() {
        changed.insert("proxy-api-key");
    }
    if args.trust_client_sampling.is_some() {
        changed.insert("trust-client-sampling");
    }
    if args.proxy_loop_detection.is_some() {
        changed.insert("proxy-loop-detection");
    }
    if args.agentic_sampling.is_some() {
        changed.insert("agentic-sampling");
    }
    if args.tool_call_repair.is_some() {
        changed.insert("tool-call-repair");
    }
    if args.proxy_autostart.is_some() {
        changed.insert("proxy-autostart");
    }
    if args.close_to_tray.is_some() {
        changed.insert("close-to-tray");
    }
    if args.start_at_login.is_some() {
        changed.insert("start-at-login");
    }

    if changed.is_empty() {
        println!("No settings provided. Use --help to see available options.");
        return Ok(());
    }

    let update = SettingsUpdate {
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
        proxy_loop_detection: args.proxy_loop_detection.map(Some),
        tool_call_repair: args.tool_call_repair.map(Some),
        agentic_sampling: args.agentic_sampling.map(Some),
        proxy_autostart: args.proxy_autostart.map(Some),
        close_to_tray: args.close_to_tray.map(Some),
        start_at_login: args.start_at_login.map(Some),
    };

    // Pre-validate: merge the prospective update into a local copy and validate
    // before persisting, so the user gets a clear error without a partial write.
    let mut prospective = ctx.app.settings().get().await?;
    if let Some(Some(v)) = &update.default_download_path {
        prospective.default_download_path = Some(v.clone());
    }
    if let Some(Some(v)) = update.default_context_size {
        prospective.default_context_size = Some(v);
    }
    if let Some(Some(v)) = update.proxy_port {
        prospective.proxy_port = Some(v);
    }
    if let Some(Some(v)) = update.llama_base_port {
        prospective.llama_base_port = Some(v);
    }
    if let Some(Some(v)) = update.max_download_queue_size {
        prospective.max_download_queue_size = Some(v);
    }
    if let Some(Some(v)) = update.max_tool_iterations {
        prospective.max_tool_iterations = Some(v);
    }
    if let Some(Some(v)) = update.max_stagnation_steps {
        prospective.max_stagnation_steps = Some(v);
    }
    if let Some(Some(v)) = update.show_memory_fit_indicators {
        prospective.show_memory_fit_indicators = Some(v);
    }
    if let Some(Some(v)) = &update.bind_host {
        prospective.bind_host = Some(v.clone());
    }
    if let Some(Some(v)) = update.share_lan {
        prospective.share_lan = Some(v);
    }
    if let Some(Some(v)) = &update.proxy_api_key {
        prospective.proxy_api_key = Some(v.clone());
    }
    if let Some(Some(v)) = update.trust_client_sampling {
        prospective.trust_client_sampling = Some(v);
    }
    if let Some(Some(v)) = update.proxy_loop_detection {
        prospective.proxy_loop_detection = Some(v);
    }
    if let Some(Some(v)) = update.agentic_sampling {
        prospective.agentic_sampling = Some(v);
    }
    if let Some(Some(v)) = update.tool_call_repair {
        prospective.tool_call_repair = Some(v);
    }
    if let Some(Some(v)) = update.proxy_autostart {
        prospective.proxy_autostart = Some(v);
    }
    if let Some(Some(v)) = update.close_to_tray {
        prospective.close_to_tray = Some(v);
    }
    if let Some(Some(v)) = update.start_at_login {
        prospective.start_at_login = Some(v);
    }
    validate_settings(&prospective)?;

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
