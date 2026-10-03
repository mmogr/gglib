//! What a command does to a model, as the table in core names it.
//!
//! A `#[path]` child of `target.rs`. Whether `--remote` reaches a command
//! that works on a model is not decided here: each such command is mapped to
//! the [`ModelAction`] it is, and the paired machine allows it or not by
//! [`ModelAction::on_paired`], ADR 0013's use-don't-change line written once
//! in core. The web page reads the same table, so a command and a button
//! cannot disagree about it.

use gglib_core::domain::ModelAction;

use super::Reach;
use crate::model_commands::ModelCommand;

/// What `--remote` does to a command that is `action`.
pub(super) const fn reach_of(action: ModelAction) -> Reach {
    if action.on_paired() {
        Reach::Use
    } else {
        Reach::Local
    }
}

/// A `gglib model` subcommand, as its refusal names it, and what it does
/// to a model.
///
/// Exhaustive, so a subcommand added to [`ModelCommand`] does not compile
/// until it has said what it is. Reading the list and reading one model use
/// a machine; everything else changes this machine's library, or reads
/// something only this machine has (its profiles, `HuggingFace` through its
/// token), and is [`ModelAction::Manage`].
pub(super) const fn model_action(command: &ModelCommand) -> (&'static str, ModelAction) {
    match command {
        ModelCommand::List(_) => ("model list", ModelAction::List),
        ModelCommand::Inspect { .. } => ("model inspect", ModelAction::Detail),
        ModelCommand::Add { .. }
        | ModelCommand::Remove { .. }
        | ModelCommand::Update { .. }
        | ModelCommand::Retag { .. }
        | ModelCommand::Verify { .. }
        | ModelCommand::Repair { .. }
        | ModelCommand::Download { .. }
        | ModelCommand::CheckUpdates { .. }
        | ModelCommand::Upgrade { .. }
        | ModelCommand::Search { .. }
        | ModelCommand::Browse { .. }
        | ModelCommand::Capabilities { .. }
        | ModelCommand::Explain { .. } => ("model", ModelAction::Manage),
    }
}
