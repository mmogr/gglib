//! The settings a session saves on its conversation, which the daemon reads
//! on each of its turns.
//!
//! A new session saves what its command line names, with the model as its
//! machine resolved it. A resumed one keeps what the conversation saved, with
//! what this command line names in its place. Split from `mod.rs`, which
//! orchestrates a session. Which machine a resume goes back to is
//! `resume_machine`'s; what a resume reprints is `memory_jogger`'s.

use anyhow::Result;
use gglib_core::domain::InferenceProfile;
use gglib_core::domain::chat::ConversationSettings;

use super::persistence::Conversation;
use crate::bootstrap::CliContext;
use crate::conversation_settings::ConversationSettingsBuilder;
use crate::handlers::inference::chat::ChatArgs;
use crate::target::TurnModel;

#[path = "resume_machine.rs"]
mod resume_machine;
pub(crate) use resume_machine::{follow_stored_machine, resumed_settings};

/// The settings a new session saves: the model as its machine resolved it,
/// the sampling and tool flags, and the profile the session samples with —
/// one configured here, or the one the paired machine routed its suffix to.
pub(crate) fn session_settings(
    args: &ChatArgs,
    profile: Option<&InferenceProfile>,
    turn: &TurnModel,
) -> ConversationSettings {
    ConversationSettingsBuilder::new(&args.sampling)
        .model_name(&turn.name)
        .model(turn.model_ref.clone())
        .profile(session_profile(profile, turn))
        .tools(args.tools.clone(), args.no_tools)
        .agent_params(args.max_iterations, args.tool_timeout_ms, args.max_parallel)
        .build()
}

/// `saved`, with each setting this command line names in its place: a
/// sampling flag, `--tools`, `--no-tools`, a limit, and a profile it
/// selected. What it does not name stays as the conversation saved it.
pub(crate) fn named_over(
    mut saved: ConversationSettings,
    args: &ChatArgs,
    profile: Option<&InferenceProfile>,
    turn: &TurnModel,
) -> ConversationSettings {
    let flags = session_settings(args, profile, turn);
    keep(flags.temperature, &mut saved.temperature);
    keep(flags.top_p, &mut saved.top_p);
    keep(flags.top_k, &mut saved.top_k);
    keep(flags.max_tokens, &mut saved.max_tokens);
    keep(flags.repeat_penalty, &mut saved.repeat_penalty);
    keep(flags.profile, &mut saved.profile);
    keep(flags.no_tools, &mut saved.no_tools);
    keep(flags.max_iterations, &mut saved.max_iterations);
    keep(flags.tool_timeout_ms, &mut saved.tool_timeout_ms);
    keep(flags.max_parallel, &mut saved.max_parallel);
    if !flags.tools.is_empty() {
        saved.tools = flags.tools;
    }
    saved
}

/// `named` in place of `held`, when it is named.
fn keep<T>(named: Option<T>, held: &mut Option<T>) {
    if named.is_some() {
        *held = named;
    }
}

/// The profile a session samples with, by name: one configured here, or
/// the one the paired machine routed its suffix to.
fn session_profile(profile: Option<&InferenceProfile>, turn: &TurnModel) -> Option<String> {
    profile
        .map(|p| p.name.clone())
        .or_else(|| turn.far_profile.clone())
}

/// Create a new conversation for a fresh session on `turn`'s model.
///
/// # Errors
///
/// A model this machine's catalogue does not hold, which no turn could run
/// on, and a write that failed.
pub(super) async fn new_conversation<'a>(
    ctx: &'a CliContext,
    args: &ChatArgs,
    profile: Option<&InferenceProfile>,
    turn: &TurnModel,
) -> Result<Conversation<'a>> {
    require_a_model(ctx, turn).await?;
    let settings = session_settings(args, profile, turn);
    let machine = turn.machine.clone();
    let history = ctx.app.chat_history();
    Conversation::create(history, args.system_prompt.clone(), settings, machine).await
}

/// Refuse a turn whose model this machine's catalogue does not hold, in the
/// words every command that needs a model refuses one with: the daemon
/// would pick another model for it.
///
/// # Errors
///
/// That refusal, naming the model as it was typed.
pub(crate) async fn require_a_model(ctx: &CliContext, turn: &TurnModel) -> Result<()> {
    if turn.model_ref.is_none() {
        let action = gglib_core::domain::ModelAction::Chat;
        crate::handlers::model::resolver::resolve_for(ctx, &turn.identifier, action).await?;
        anyhow::bail!("no model here is named or numbered '{}'", turn.identifier);
    }
    Ok(())
}

#[cfg(test)]
#[path = "resume_limits_tests.rs"]
mod resume_limits_tests;
#[cfg(test)]
#[path = "resume_rows_tests.rs"]
mod resume_rows_tests;
#[cfg(test)]
#[path = "resume_settings_tests.rs"]
pub(super) mod tests;
