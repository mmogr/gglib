//! Configuration, tooling, and system management handlers.
//!
//! Dispatches [`ConfigCommand`] variants to focused sub-modules:
//! settings/default/models-dir, llama.cpp lifecycle, assistant-ui,
//! dependency checks, and resolved-path inspection.

pub mod assistant_ui;
pub mod check_deps;
pub mod llama;
pub mod llama_install;
pub mod paths;
pub mod settings;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::config_commands::ConfigCommand;

/// Dispatch a `config` subcommand to its handler.
pub async fn dispatch(ctx: &CliContext, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Default { identifier, clear } => {
            settings::handle_default_model(ctx, identifier, clear).await
        }
        ConfigCommand::ModelsDir { command } => settings::handle_models_dir(command),
        ConfigCommand::Settings { command } => settings::handle_settings(ctx, command).await,
        ConfigCommand::Llama { command } => llama::dispatch(command).await,
        ConfigCommand::AssistantUi { command } => assistant_ui::dispatch(command),
        ConfigCommand::CheckDeps => {
            let probe = gglib_runtime::DefaultSystemProbe::new();
            check_deps::execute(&probe).await
        }
        ConfigCommand::Paths => paths::execute(),
    }
}
