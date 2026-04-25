//! Settings, default-model, and models-directory handlers.
//!
//! These three sub-handlers were the original `config` dispatch targets.
//! They now live inside the `config/` directory alongside llama, assistant-ui,
//! check-deps, and paths.
//!
//! All display/formatting logic is in the sibling [`settings_display`] module.

mod settings_display;

use std::collections::BTreeSet;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::config_commands::{ModelsDirCommand, SettingsCommand};
use crate::utils::input::prompt_string_with_default;
use gglib_core::paths::{
    DirectoryCreationStrategy, default_models_dir, ensure_directory, persist_models_dir,
    resolve_models_dir,
};
use gglib_core::{Settings, SettingsUpdate, validate_settings};

use settings_display::{
    print_display_rows, print_sections, settings_display_rows, settings_to_sections,
};

/// Resolve the display string for `default-model-id`, performing a DB lookup when set.
///
/// Returns `Some("42 (ModelName)")`, `Some("42 (not found)")`, or `None`.
async fn resolve_model_display(ctx: &CliContext, settings: &Settings) -> Result<Option<String>> {
    match settings.default_model_id {
        Some(model_id) => match ctx.app.models().get_by_id(model_id).await? {
            Some(model) => Ok(Some(format!("{} ({})", model_id, model.name))),
            None => Ok(Some(format!("{} (not found)", model_id))),
        },
        None => Ok(None),
    }
}

/// Handle the `config default` command for managing the default model.
///
/// - No args: show current default
/// - With identifier: set as default
/// - With --clear: remove default
pub async fn handle_default_model(
    ctx: &CliContext,
    identifier: Option<String>,
    clear: bool,
) -> Result<()> {
    if clear {
        // Clear the default model
        let update = SettingsUpdate {
            default_model_id: Some(None),
            ..Default::default()
        };
        ctx.app.settings().update(update).await?;
        println!("✓ Default model cleared.");
        return Ok(());
    }

    match identifier {
        Some(id) => {
            // Set the default model
            let model = ctx.app.models().find_by_identifier(&id).await?;
            let update = SettingsUpdate {
                default_model_id: Some(Some(model.id)),
                ..Default::default()
            };
            ctx.app.settings().update(update).await?;
            println!("✓ Default model set to: {} (ID: {})", model.name, model.id);
        }
        None => {
            // Show current default
            let settings = ctx.app.settings().get().await?;
            match settings.default_model_id {
                Some(model_id) => match ctx.app.models().get_by_id(model_id).await? {
                    Some(model) => {
                        println!("Default model: {} (ID: {})", model.name, model.id);
                    }
                    None => {
                        println!("Default model ID: {} (warning: model not found)", model_id);
                    }
                },
                None => {
                    println!("No default model set.");
                    println!("Use 'gglib config default <id-or-name>' to set one.");
                }
            }
        }
    }
    Ok(())
}

pub fn handle_models_dir(command: ModelsDirCommand) -> Result<()> {
    match command {
        ModelsDirCommand::Show => {
            let resolved = resolve_models_dir(None)?;
            println!(
                "Current models directory: {} (source: {:?})",
                resolved.path.display(),
                resolved.source
            );
            Ok(())
        }
        ModelsDirCommand::Prompt => {
            let default_path = default_models_dir()?.to_string_lossy().to_string();
            let answer = prompt_string_with_default(
                "Where should gglib store downloaded models?",
                Some(&default_path),
            )?;
            let resolved = resolve_models_dir(Some(&answer))?;
            ensure_directory(&resolved.path, DirectoryCreationStrategy::AutoCreate)?;
            persist_models_dir(&resolved.path)?;
            println!(
                "✓ Models directory updated to {} (interactive)",
                resolved.path.display()
            );
            Ok(())
        }
        ModelsDirCommand::Set { path, no_create } => {
            let resolved = resolve_models_dir(Some(&path))?;
            let strategy = if no_create {
                DirectoryCreationStrategy::Disallow
            } else {
                DirectoryCreationStrategy::AutoCreate
            };
            ensure_directory(&resolved.path, strategy)?;
            persist_models_dir(&resolved.path)?;
            println!(
                "✓ Models directory updated to {} (non-interactive)",
                resolved.path.display()
            );
            Ok(())
        }
    }
}

pub async fn handle_settings(ctx: &CliContext, command: SettingsCommand) -> Result<()> {
    match command {
        SettingsCommand::Show => {
            let settings = ctx.app.settings().get().await?;
            let model_display = resolve_model_display(ctx, &settings).await?;
            let rows = settings_display_rows(&settings, model_display);
            println!("Current application settings:");
            print_sections(&settings_to_sections(&rows));
            Ok(())
        }
        SettingsCommand::Set {
            default_context_size,
            proxy_port,
            llama_base_port,
            max_download_queue_size,
            default_download_path,
            max_tool_iterations,
            max_stagnation_steps,
            show_memory_fit_indicators,
        } => {
            // Collect the kebab-case keys of every flag that was provided.
            let mut changed: BTreeSet<&str> = BTreeSet::new();
            if default_download_path.is_some() {
                changed.insert("default-download-path");
            }
            if default_context_size.is_some() {
                changed.insert("default-context-size");
            }
            if proxy_port.is_some() {
                changed.insert("proxy-port");
            }
            if llama_base_port.is_some() {
                changed.insert("llama-base-port");
            }
            if max_download_queue_size.is_some() {
                changed.insert("max-download-queue-size");
            }
            if max_tool_iterations.is_some() {
                changed.insert("max-tool-iterations");
            }
            if max_stagnation_steps.is_some() {
                changed.insert("max-stagnation-steps");
            }
            if show_memory_fit_indicators.is_some() {
                changed.insert("show-memory-fit-indicators");
            }

            if changed.is_empty() {
                println!("No settings provided. Use --help to see available options.");
                return Ok(());
            }

            let update = SettingsUpdate {
                default_download_path: default_download_path.map(Some),
                default_context_size: default_context_size.map(Some),
                proxy_port: proxy_port.map(Some),
                llama_base_port: llama_base_port.map(Some),
                max_download_queue_size: max_download_queue_size.map(Some),
                show_memory_fit_indicators: show_memory_fit_indicators.map(Some),
                max_tool_iterations: max_tool_iterations.map(Some),
                max_stagnation_steps: max_stagnation_steps.map(Some),
                default_model_id: None,
                inference_defaults: None,
                setup_completed: None,
                title_generation_prompt: None,
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
        SettingsCommand::Reset { force } => {
            if !force {
                let confirm = crate::utils::input::prompt_confirmation(
                    "Are you sure you want to reset all settings to defaults?",
                )?;
                if !confirm {
                    println!("Reset cancelled.");
                    return Ok(());
                }
            }

            let defaults = Settings::with_defaults();
            ctx.app.settings().save(&defaults).await?;
            println!("✓ All settings have been reset to defaults.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_config_handler_exists() {
        // Placeholder test — substantive tests live in settings_display.rs.
    }
}
