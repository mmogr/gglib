//! Which commands wait out Hugging Face's rate limit, and which surface it.
//!
//! A one-shot command that asks the Hub in this process, and is over once
//! it has its answer, waits: its Hub client is built with
//! [`OnLimit::Wait`], told through the status line ([`hub_wait_notice`]).
//! Ctrl-C ends the wait by ending the command, since none of these commands
//! listens for it before its first Hub reads are done. `up`'s download
//! monitor does listen, so a model queued from it with `a` holds a Ctrl-C
//! until its wait is over, fifteen minutes at most.
//!
//! Every other command keeps [`OnLimit::Surface`]: one that answers
//! requests (`daemon run`, `web`, `serve`, `proxy`, `chat`, `mcp`) leaves
//! the waiting to whoever asked it, and one that asks through the daemon is
//! answered by the daemon's client.

use std::sync::Arc;

use gglib_core::hub_limit::OnLimit;

use crate::commands::Commands;
use crate::console::CliConsole;
use crate::model_commands::ModelCommand;
use crate::presentation::status_line::hub_wait_notice;

/// What `command`'s Hub client does at the Hub's rate limit; a wait is
/// drawn on `console`.
pub(crate) fn on_hub_limit(command: &Commands, console: &Arc<CliConsole>) -> OnLimit {
    if waits_on_the_hub(command) {
        OnLimit::Wait(hub_wait_notice(Arc::clone(console)))
    } else {
        OnLimit::Surface
    }
}

/// Whether `command` waits the Hub's rate limit out: `up`, and the `model`
/// commands that ask the Hub in this process (`search`, `browse`,
/// `download`'s variant listing and the companions it names before it
/// queues, `check-updates` and `upgrade`).
fn waits_on_the_hub(command: &Commands) -> bool {
    match command {
        Commands::Up { .. } => true,
        Commands::Model { command } => matches!(
            command,
            ModelCommand::Search { .. }
                | ModelCommand::Browse { .. }
                | ModelCommand::Download { .. }
                | ModelCommand::CheckUpdates { .. }
                | ModelCommand::Upgrade { .. }
        ),
        _ => false,
    }
}

#[cfg(test)]
#[path = "hub_wait_tests.rs"]
mod tests;
