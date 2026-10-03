//! Restoring a prior session's settings, and showing the user where they left
//! off.
//!
//! Both halves of "resuming feels like continuing": the saved
//! [`ConversationSettings`] fill in whatever this invocation did not state,
//! and the jogger reprints the tail of the conversation so the first new turn
//! has visible context. Split from `mod.rs`, which orchestrates a session —
//! merging stored settings and formatting a recap are a different job, and the
//! file had reached its size budget. What a new session saves for a later
//! resume is built here too, so the two sides are read together. Which
//! machine a resume goes back to, and what it saves, is `resume_machine`'s.

use gglib_core::domain::InferenceProfile;
use gglib_core::domain::chat::ConversationSettings;

use crate::conversation_settings::ConversationSettingsBuilder;
use crate::handlers::inference::chat::ChatArgs;
use crate::handlers::inference::profile_selection::warn_profile_gone;
use crate::presentation::style;
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
/// CLI-provided values always win; saved settings fill in blanks.
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

/// Print the last user/assistant exchange as a memory jogger when resuming.
pub(crate) fn print_memory_jogger(db_messages: &[gglib_core::domain::chat::Message], title: &str) {
    use gglib_core::domain::chat::MessageRole;

    println!("\n{}Resuming: {}{}\n", style::INFO, title, style::RESET);

    // Find last user message and last assistant message
    let last_user = db_messages
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::User);
    let last_assistant = db_messages
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::Assistant);

    if let Some(user_msg) = last_user {
        let content = if user_msg.content.len() > 200 {
            format!("{}…", &user_msg.content[..200])
        } else {
            user_msg.content.clone()
        };
        println!("{}  You: {}{}", style::DIM, content, style::RESET);
    }
    if let Some(asst_msg) = last_assistant {
        let content = if asst_msg.content.len() > 200 {
            format!("{}…", &asst_msg.content[..200])
        } else {
            asst_msg.content.clone()
        };
        println!("{}  Assistant: {}{}", style::DIM, content, style::RESET);
    }
    println!();
}

#[cfg(test)]
#[path = "resume_settings_tests.rs"]
mod tests;
