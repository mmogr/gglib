use crate::cli::{ConfigCommand, ModelsDirCommand, SettingsCommand};
use crate::services::core::AppCore;
use crate::services::settings::{Settings, SettingsUpdate, validate_settings};
use crate::utils::input;
use crate::utils::paths::{
    DirectoryCreationStrategy, default_models_dir, ensure_directory, persist_models_dir,
    resolve_models_dir,
};
use anyhow::Result;
use std::sync::Arc;

/// Entry point for `gglib config` commands.
pub fn handle(core: Arc<AppCore>, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::ModelsDir { command } => handle_models_dir(command),
        ConfigCommand::Settings { command } => {
            // Settings commands need async runtime for database access
            tokio::runtime::Runtime::new()?.block_on(handle_settings(core, command))
        }
    }
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
            let answer = input::prompt_string_with_default(
                "Where should gglib store downloaded models?",
                Some(&default_path),
            )?;
            let resolved = resolve_models_dir(Some(&answer))?;
            ensure_directory(&resolved.path, DirectoryCreationStrategy::PromptUser)?;
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

async fn handle_settings(core: Arc<AppCore>, command: SettingsCommand) -> Result<()> {
    match command {
        SettingsCommand::Show => {
            let settings = core.settings().get().await?;
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
            println!("  server_port:             {:?}", settings.server_port);
            println!(
                "  max_download_queue_size: {:?}",
                settings.max_download_queue_size
            );
            Ok(())
        }
        SettingsCommand::Set {
            default_context_size,
            proxy_port,
            server_port,
            max_download_queue_size,
            default_download_path,
        } => {
            // Check if any settings were provided and which ones
            let has_default_download_path = default_download_path.is_some();
            let has_default_context_size = default_context_size.is_some();
            let has_proxy_port = proxy_port.is_some();
            let has_server_port = server_port.is_some();
            let has_max_download_queue_size = max_download_queue_size.is_some();

            if !has_default_download_path
                && !has_default_context_size
                && !has_proxy_port
                && !has_server_port
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
                server_port: server_port.map(Some),
                max_download_queue_size: max_download_queue_size.map(Some),
                show_memory_fit_indicators: None,
            };

            // Get current settings and apply updates for validation
            let mut current = core.settings().get().await?;
            if let Some(Some(v)) = &update.default_download_path {
                current.default_download_path = Some(v.clone());
            }
            if let Some(Some(v)) = update.default_context_size {
                current.default_context_size = Some(v);
            }
            if let Some(Some(v)) = update.proxy_port {
                current.proxy_port = Some(v);
            }
            if let Some(Some(v)) = update.server_port {
                current.server_port = Some(v);
            }
            if let Some(Some(v)) = update.max_download_queue_size {
                current.max_download_queue_size = Some(v);
            }

            // Validate before saving
            validate_settings(&current)?;

            // Save settings
            let updated = core.settings().update(update).await?;
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
            if has_server_port {
                println!("  server_port: {:?}", updated.server_port);
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
                let confirm = input::prompt_confirmation("Reset all settings to defaults?")?;
                if !confirm {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let defaults = Settings::default();
            core.settings().save(&defaults).await?;
            println!("✓ Settings reset to defaults:");
            println!(
                "  default_download_path:   {:?}",
                defaults.default_download_path
            );
            println!(
                "  default_context_size:    {:?}",
                defaults.default_context_size
            );
            println!("  proxy_port:              {:?}", defaults.proxy_port);
            println!("  server_port:             {:?}", defaults.server_port);
            println!(
                "  max_download_queue_size: {:?}",
                defaults.max_download_queue_size
            );
            Ok(())
        }
    }
}
