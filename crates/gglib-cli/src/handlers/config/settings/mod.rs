#![doc = include_str!("README.md")]
pub(crate) mod profiles;
mod set;
mod settings_display;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::config_commands::{ModelsDirCommand, SettingsCommand};
use crate::utils::input::prompt_string_with_default;
use gglib_core::paths::{
    DirectoryCreationStrategy, default_models_dir, ensure_directory, persist_models_dir,
    resolve_models_dir,
};
use gglib_core::{Settings, SettingsUpdate};

use settings_display::{print_sections, settings_display_rows, settings_to_sections};

pub(crate) use profiles::handle_profile;

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
pub(crate) async fn handle_default_model(
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

pub(crate) fn handle_models_dir(command: ModelsDirCommand) -> Result<()> {
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

pub(crate) async fn handle_settings(ctx: &CliContext, command: SettingsCommand) -> Result<()> {
    match command {
        SettingsCommand::Show => {
            let settings = ctx.app.settings().get().await?;
            let model_display = resolve_model_display(ctx, &settings).await?;
            let rows = settings_display_rows(&settings, model_display);
            println!("Current application settings:");
            print_sections(&settings_to_sections(&rows));
            Ok(())
        }
        SettingsCommand::Set(args) => set::handle_set(ctx, *args).await,
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
