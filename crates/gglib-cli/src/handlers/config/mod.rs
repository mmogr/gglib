#![doc = include_str!("README.md")]
pub(crate) mod check_deps;
pub(crate) mod fast_downloads;
pub(crate) mod llama;
pub(crate) mod llama_detect;
#[allow(
    clippy::fn_params_excessive_bools,
    clippy::literal_string_with_formatting_args,
    clippy::trivially_copy_pass_by_ref,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod llama_install;
pub(crate) mod llama_prebuilt;
pub(crate) mod paths;
pub(crate) mod settings;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::config_commands::ConfigCommand;

/// Dispatch a `config` subcommand to its handler.
pub(crate) async fn dispatch(ctx: &CliContext, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Default { identifier, clear } => {
            settings::handle_default_model(ctx, identifier, clear).await
        }
        ConfigCommand::ModelsDir { command } => settings::handle_models_dir(command),
        ConfigCommand::Settings { command } => settings::handle_settings(ctx, command).await,
        ConfigCommand::Profile { command } => settings::handle_profile(ctx, command).await,
        ConfigCommand::Llama { command } => llama::dispatch(command).await,
        ConfigCommand::CheckDeps {
            setup_fast_downloads,
        } => {
            let probe = gglib_runtime::DefaultSystemProbe::new();
            check_deps::execute(&probe, setup_fast_downloads).await
        }
        ConfigCommand::FastDownloads { command } => fast_downloads::dispatch(command).await,
        ConfigCommand::Paths => paths::execute(),
    }
}
