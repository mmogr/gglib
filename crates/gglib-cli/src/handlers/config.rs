//! Config command handler.
//!
//! Handles configuration management including models directory and settings.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::config_commands::{ConfigCommand, ModelsDirCommand, SettingsCommand};
use crate::utils::input::prompt_string_with_default;
use gglib_core::paths::{
    DirectoryCreationStrategy, default_models_dir, ensure_directory, persist_models_dir,
    resolve_models_dir,
};
use gglib_core::{Settings, SettingsUpdate, validate_settings};

/// Execute the config command.
///
/// Dispatches to the appropriate subcommand handler.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
/// * `command` - The config subcommand to execute
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
pub async fn execute(ctx: &CliContext, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Default { identifier, clear } => {
            handle_default_model(ctx, identifier, clear).await
        }
        ConfigCommand::ModelsDir { command } => handle_models_dir(command),
        ConfigCommand::Settings { command } => handle_settings(ctx, command).await,
    }
}

/// Handle the `config default` command for managing the default model.
///
/// - No args: show current default
/// - With identifier: set as default
/// - With --clear: remove default
async fn handle_default_model(
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
        ctx.app().settings().update(update).await?;
        println!("✓ Default model cleared.");
        return Ok(());
    }

    match identifier {
        Some(id) => {
            // Set the default model
            let model = ctx.app().models().find_by_identifier(&id).await?;
            let update = SettingsUpdate {
                default_model_id: Some(Some(model.id)),
                ..Default::default()
            };
            ctx.app().settings().update(update).await?;
            println!("✓ Default model set to: {} (ID: {})", model.name, model.id);
        }
        None => {
            // Show current default
            let settings = ctx.app().settings().get().await?;
            match settings.default_model_id {
                Some(model_id) => {
                    match ctx.app().models().get_by_id(model_id).await? {
                        Some(model) => {
                            println!("Default model: {} (ID: {})", model.name, model.id);
                        }
                        None => {
                            println!("Default model ID: {} (warning: model not found)", model_id);
                        }
                    }
                }
                None => {
                    println!("No default model set.");
                    println!("Use 'gglib config default <id-or-name>' to set one.");
                }
            }
        }
    }
    Ok(())
}

fn handle_models_dir(command: ModelsDirCommand) -> Result<()> {
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

async fn handle_settings(ctx: &CliContext, command: SettingsCommand) -> Result<()> {
    match command {
        SettingsCommand::Show => {
            let settings = ctx.app().settings().get().await?;
            println!("Current application settings:");
            println!(
                "  default_download_path:   {:?}",
                settings.default_download_path
            );
            println!(
                "  default_context_size:    {:?}",
                settings.default_context_size
            );
            println!("  proxy_port:              {:?}", settings.proxy_port);
            println!("  llama_base_port:         {:?}", settings.llama_base_port);
            println!(
                "  max_download_queue_size: {:?}",
                settings.max_download_queue_size
            );

            // Show default model with name if available
            match settings.default_model_id {
                Some(model_id) => match ctx.app().models().get_by_id(model_id).await? {
                    Some(model) => {
                        println!("  default_model_id:        {} ({})", model_id, model.name);
                    }
                    None => {
                        println!("  default_model_id:        {} (not found)", model_id);
                    }
                },
                None => {
                    println!("  default_model_id:        None");
                }
            }
            Ok(())
        }
        SettingsCommand::Set {
            default_context_size,
            proxy_port,
            llama_base_port,
            max_download_queue_size,
            default_download_path,
        } => {
            // Check if any settings were provided
            let has_default_download_path = default_download_path.is_some();
            let has_default_context_size = default_context_size.is_some();
            let has_proxy_port = proxy_port.is_some();
            let has_llama_base_port = llama_base_port.is_some();
            let has_max_download_queue_size = max_download_queue_size.is_some();

            if !has_default_download_path
                && !has_default_context_size
                && !has_proxy_port
                && !has_llama_base_port
                && !has_max_download_queue_size
            {
                println!("No settings provided. Use --help to see available options.");
                return Ok(());
            }

            // Build update request
            let update = SettingsUpdate {
                default_download_path: default_download_path.map(Some),
                default_context_size: default_context_size.map(Some),
                proxy_port: proxy_port.map(Some),
                llama_base_port: llama_base_port.map(Some),
                max_download_queue_size: max_download_queue_size.map(Some),
                show_memory_fit_indicators: None,
                max_tool_iterations: None,
                max_stagnation_steps: None,
                default_model_id: None,
            };

            // Get current settings and apply updates for validation
            let mut current = ctx.app().settings().get().await?;
            if let Some(Some(v)) = &update.default_download_path {
                current.default_download_path = Some(v.clone());
            }
            if let Some(Some(v)) = update.default_context_size {
                current.default_context_size = Some(v);
            }
            if let Some(Some(v)) = update.proxy_port {
                current.proxy_port = Some(v);
            }
            if let Some(Some(v)) = update.llama_base_port {
                current.llama_base_port = Some(v);
            }
            if let Some(Some(v)) = update.max_download_queue_size {
                current.max_download_queue_size = Some(v);
            }

            // Validate before saving
            validate_settings(&current)?;

            // Save settings
            let updated = ctx.app().settings().update(update).await?;
            println!("✓ Settings updated successfully:");
            if has_default_download_path {
                println!(
                    "  default_download_path: {:?}",
                    updated.default_download_path
                );
            }
            if has_default_context_size {
                println!("  default_context_size: {:?}", updated.default_context_size);
            }
            if has_proxy_port {
                println!("  proxy_port: {:?}", updated.proxy_port);
            }
            if has_llama_base_port {
                println!("  llama_base_port: {:?}", updated.llama_base_port);
            }
            if has_max_download_queue_size {
                println!(
                    "  max_download_queue_size: {:?}",
                    updated.max_download_queue_size
                );
            }
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
            ctx.app().settings().save(&defaults).await?;
            println!("✓ All settings have been reset to defaults.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_config_handler_exists() {
        // Placeholder test to ensure module compiles
    }
}
