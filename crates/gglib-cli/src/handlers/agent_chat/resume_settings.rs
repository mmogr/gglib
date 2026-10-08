//! Restoring a prior session's settings.
//!
//! The saved [`ConversationSettings`] fill in whatever this invocation did
//! not state. Split from `mod.rs`, which orchestrates a session — merging
//! stored settings is a different job, and the file had reached its size
//! budget. What a new session saves for a later resume is built here too,
//! with the conversation it saves it on, so the two sides are read together.
//! The chat's Thinking choice is settled here as well ([`settle_thinking`]),
//! against the command line, for a new session and a resumed one.
//! Which machine a resume goes back to, and what it saves, is
//! `resume_machine`'s; what a resume reprints is `memory_jogger`'s.

use gglib_core::domain::chat::ConversationSettings;
use gglib_core::domain::thinking::{self, Remember};
use gglib_core::domain::{InferenceProfile, Thinking};

use super::persistence::Conversation;
use crate::bootstrap::CliContext;
use crate::conversation_settings::ConversationSettingsBuilder;
use crate::handlers::inference::chat::ChatArgs;
use crate::handlers::inference::profile_selection::warn_profile_gone;
use crate::target::{Target, TurnModel};

#[path = "resume_machine.rs"]
mod resume_machine;
pub(crate) use resume_machine::{follow_stored_machine, resumed_settings};

/// The settings a new session saves, so `--continue` can restore them: the
/// model as its machine resolved it, the sampling and tool flags, and the
/// profile the session samples with — one configured here, or the one the
/// paired machine routed its suffix to.
pub(crate) fn session_settings(
    args: &ChatArgs,
    profile: Option<&InferenceProfile>,
    turn: &TurnModel,
) -> ConversationSettings {
    ConversationSettingsBuilder::new(&args.sampling, &args.context)
        .model_name(&turn.name)
        .model(turn.model_ref.clone())
        .profile(session_profile(profile, turn))
        .tools(args.tools.clone(), args.no_tools)
        .agent_params(args.max_iterations, args.tool_timeout_ms, args.max_parallel)
        .build()
}

/// The profile a session samples with, by name: one configured here, or
/// the one the paired machine routed its suffix to.
fn session_profile(profile: Option<&InferenceProfile>, turn: &TurnModel) -> Option<String> {
    profile
        .map(|p| p.name.clone())
        .or_else(|| turn.far_profile.clone())
}

/// The profile a resumed session samples with.
///
/// One this invocation selected — `--profile`, or a suffix typed or replayed —
/// wins. Otherwise the one the conversation was saved with fills in, as a
/// saved temperature does. A saved profile since deleted resumes without one,
/// with the warning a deleted suffix gets, so the conversation stays
/// resumable. Nothing is restored on the paired machine: a profile configured
/// here does not say how that machine samples.
pub(crate) fn restore_profile(
    selected: Option<InferenceProfile>,
    target: Target,
    profiles: &[InferenceProfile],
    saved: Option<&str>,
) -> Option<InferenceProfile> {
    if selected.is_some() || target == Target::Remote {
        return selected;
    }
    let name = saved?;
    let found = profiles.iter().find(|p| p.name == name).cloned();
    if found.is_none() {
        warn_profile_gone(name);
    }
    found
}

/// Merge saved [`ConversationSettings`] into [`ChatArgs`].
///
/// CLI-provided values always win; saved settings fill in blanks. The chat's
/// Thinking choice is not merged here: [`settle_thinking`] reads it.
#[allow(
    clippy::assigning_clones,
    clippy::ref_option,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) fn apply_saved_settings(
    args: &ChatArgs,
    saved_system_prompt: &Option<String>,
    saved_settings: &Option<ConversationSettings>,
) -> ChatArgs {
    let mut merged = args.clone();

    // Restore system prompt if the user didn't supply one on the CLI.
    if merged.system_prompt.is_none() {
        merged.system_prompt.clone_from(saved_system_prompt);
    }

    let Some(saved) = saved_settings else {
        return merged;
    };

    // Model identifier: CLI wins if non-empty, otherwise use saved.
    if merged.identifier.is_empty()
        && let Some(ref name) = saved.model_name
    {
        merged.identifier = name.clone();
    }

    // Sampling parameters — only fill if CLI left them as None.
    if merged.sampling.temperature.is_none() {
        merged.sampling.temperature = saved.temperature;
    }
    if merged.sampling.top_p.is_none() {
        merged.sampling.top_p = saved.top_p;
    }
    if merged.sampling.top_k.is_none() {
        merged.sampling.top_k = saved.top_k;
    }
    if merged.sampling.max_tokens.is_none() {
        merged.sampling.max_tokens = saved.max_tokens;
    }
    if merged.sampling.repeat_penalty.is_none() {
        merged.sampling.repeat_penalty = saved.repeat_penalty;
    }

    // Context args
    if merged.context.ctx_size.is_none() {
        merged.context.ctx_size.clone_from(&saved.ctx_size);
    }
    if !merged.context.mlock {
        merged.context.mlock = saved.mlock.unwrap_or(false);
    }

    // Tools — only restore if the user didn't provide any on the CLI.
    if merged.tools.is_empty() {
        merged.tools.clone_from(&saved.tools);
    }
    if !merged.no_tools {
        merged.no_tools = saved.no_tools.unwrap_or(false);
    }

    // Agent loop params — fill if the user didn't override.
    if merged.max_iterations.is_none()
        && let Some(saved_max) = saved.max_iterations
    {
        merged.max_iterations = Some(saved_max);
    }
    if merged.tool_timeout_ms.is_none() {
        merged.tool_timeout_ms = saved.tool_timeout_ms;
    }
    if merged.max_parallel.is_none() {
        merged.max_parallel = saved.max_parallel;
    }

    merged
}

/// Settle a session's Thinking choice by the rule the daemon reads a turn by
/// ([`thinking::settle`]): what `--thinking` named, against what the chat
/// `remembered` (nothing, for a new chat) and the budget
/// `--reasoning-budget-tokens` typed. The budget the session runs with goes
/// into `args`, and what the chat is to remember comes back.
///
/// A named choice wins: `off` runs with a budget of `0`, `on` with the budget
/// typed. With none named the chat runs as it remembers: switched off, its
/// budget is `0` whatever was typed, and the session says so when that sets
/// a typed budget aside.
pub(crate) fn settle_thinking(args: &mut ChatArgs, remembered: Option<Thinking>) -> Remember {
    let typed = args.sampling.reasoning_budget_tokens;
    let settled = thinking::settle(args.thinking, remembered, typed);
    // With nothing named the rule changes a budget only for a chat switched
    // off, so a typed budget that is not the one settled was set aside by a
    // choice this command line did not make.
    if args.thinking.is_none()
        && let Some(typed) = typed.filter(|_| settled.budget != typed)
    {
        note_budget_set_aside(typed);
    }
    args.sampling.reasoning_budget_tokens = settled.budget;
    settled.remember
}

/// Say that a resumed chat has Thinking switched off, that the budget its
/// command line typed was not applied, and the flag that switches it back.
///
/// On stderr, with a session's other notices, and from the one place a
/// session reads the choice: once a session, before its first turn.
fn note_budget_set_aside(typed: i32) {
    eprintln!(
        "  This chat has Thinking switched off, so --reasoning-budget-tokens {typed} was not \
         applied. Add --thinking on to switch it back on."
    );
}

/// Create a new conversation for a fresh session on `turn`'s model.
pub(super) async fn new_conversation<'a>(
    ctx: &'a CliContext,
    args: &ChatArgs,
    profile: Option<&InferenceProfile>,
    turn: &TurnModel,
) -> Option<Conversation<'a>> {
    let settings = session_settings(args, profile, turn);

    match Conversation::create(
        ctx.app.chat_history(),
        args.system_prompt.clone(),
        Some(settings),
        turn.made_by(),
    )
    .await
    {
        Ok(conv) => Some(conv),
        Err(e) => {
            tracing::warn!("failed to create agent conversation: {e}");
            None
        }
    }
}

#[cfg(test)]
#[path = "resume_limits_tests.rs"]
mod resume_limits_tests;
#[cfg(test)]
#[path = "resume_rows_tests.rs"]
mod resume_rows_tests;
#[cfg(test)]
#[path = "resume_thinking_tests.rs"]
mod resume_thinking_tests;
#[cfg(test)]
#[path = "resume_settings_tests.rs"]
pub(super) mod tests;
