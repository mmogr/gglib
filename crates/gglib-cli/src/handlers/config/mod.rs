#![doc = include_str!("README.md")]
pub(crate) mod check_deps;
pub(crate) mod engine;
pub(crate) mod fast_downloads;
pub(crate) mod llama;
pub(crate) mod llama_detect;
mod llama_events;
pub(crate) mod llama_install;
pub(crate) mod paths;
pub(crate) mod sd;
pub(crate) mod settings;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::config_commands::ConfigCommand;

/// Dispatch a `config` subcommand to its handler.
pub(crate) async fn dispatch(ctx: &CliContext, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Show => settings::handle_show(ctx).await,
        ConfigCommand::Set { key, value } => settings::handle_set(ctx, key, &value).await,
        ConfigCommand::Unset { key } => settings::handle_unset(ctx, key).await,
        ConfigCommand::Reset { yes } => settings::handle_reset(ctx, yes).await,
        ConfigCommand::Reveal { key } => settings::handle_reveal(ctx, key.0).await,
        ConfigCommand::Profile { command } => settings::handle_profile(ctx, command).await,
        ConfigCommand::Llama { command } => llama::dispatch(command).await,
        ConfigCommand::Sd { command } => sd::dispatch(command).await,
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
