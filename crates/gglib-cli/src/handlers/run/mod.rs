#![doc = include_str!("README.md")]

mod read;
mod start;
mod text;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use gglib_core::domain::runs::is_run_id;

use crate::bootstrap::CliContext;
use crate::daemon_client::ensure_daemon;

/// `gglib run`.
#[derive(Args)]
pub struct RunArgs {
    #[command(subcommand)]
    command: RunCommand,
}

/// Subcommands available under `gglib run`.
#[derive(Subcommand)]
pub(crate) enum RunCommand {
    /// Start a reply; prints its id
    Start {
        /// The model to ask, as the proxy names it
        #[arg(long, short)]
        model: String,
        /// What to ask
        prompt: String,
        /// Print the reply as it arrives instead of the id
        #[arg(long, short)]
        follow: bool,
    },
    /// List the daemon's runs, newest first
    List,
    /// Print a run's reply so far
    Show {
        /// The run's id
        id: String,
        /// Keep printing until the run ends
        #[arg(long, short)]
        follow: bool,
    },
    /// Stop a run
    Cancel {
        /// The run's id
        id: String,
    },
}

/// Route a `gglib run` subcommand.
pub(crate) async fn dispatch(ctx: &CliContext, args: RunArgs) -> Result<()> {
    match args.command {
        RunCommand::Start {
            model,
            prompt,
            follow,
        } => start::start(&ensure_daemon(ctx).await?, &model, &prompt, follow).await,
        RunCommand::List => read::list(&ensure_daemon(ctx).await?).await,
        RunCommand::Show { id, follow } => {
            checked(&id)?;
            read::show(&ensure_daemon(ctx).await?, &id, follow).await
        }
        RunCommand::Cancel { id } => {
            checked(&id)?;
            read::cancel(&ensure_daemon(ctx).await?, &id).await
        }
    }
}

/// Refuse an id that is not one before it reaches a request path, where a
/// `..` would name another route.
fn checked(id: &str) -> Result<()> {
    if !is_run_id(id) {
        bail!(
            "{id:?} is not a run id. Ids look like run-0a1b2c3d4e5f; `gglib run list` shows them."
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
