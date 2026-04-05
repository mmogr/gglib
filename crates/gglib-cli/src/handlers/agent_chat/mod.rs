//! Interactive agentic chat handler for `gglib chat`.
//!
//! Entry point: [`run`].  Sub-modules keep each concern small and
//! independently readable:
//! - [`config`]   — resolves LLM port + MCP tools, composes an [`gglib_core::ports::AgentLoopPort`]
//! - [`renderer`] — maps [`gglib_core::AgentEvent`] variants to terminal output
//! - [`drain`]    — async event-stream consumer (spinner, thinking accumulator)
//! - [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input
//! - [`tool_format`] — tool-result summary formatters
//! - [`markdown`] — Markdown normalisation + termimad rendering
//! - [`thinking_dispatch`] — `RenderContext`, thinking-event dispatch, spinner coordination

pub mod config;
pub mod drain;
mod markdown;
pub mod persistence;
pub mod renderer;
pub mod repl;
mod thinking_dispatch;
mod tool_format;

use anyhow::{bail, Result};

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::ConversationSettings;

use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;
use crate::presentation::style;
use crate::shared_args::ConversationSettingsBuilder;

use self::persistence::Conversation;

/// Entry point: start the interactive agentic REPL.
///
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
/// When `args.continue_id` is set, loads a previous conversation and resumes
/// with the original session parameters (saved settings fill in any CLI args
/// the user didn't explicitly provide).
pub async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    // 1. If resuming, load the conversation first and merge saved settings
    //    into args so the agent is composed with the correct parameters.
    let mut args = args.clone();
    let (persistence, prior_messages) = if let Some(conv_id) = args.continue_id {
        let (merged_args, conv, prior) = resume_conversation(ctx, &args, conv_id).await?;
        args = merged_args;
        (Some(conv), prior)
    } else {
        if args.identifier.is_empty() {
            bail!("model identifier is required (use --continue <ID> to resume a session)");
        }
        let (conv, prior) = new_conversation(ctx, &args).await;
        (conv, prior)
    };

    // 2. Compose the agent with the (possibly merged) args.
    let inference_config = args.sampling.clone().into_inference_config();
    let sampling = if inference_config == Default::default() {
        None
    } else {
        Some(inference_config)
    };
    let params = config::AgentSessionParams::from(&args);
    let (agent, maybe_handle) = config::compose(ctx, &params, None, sampling).await?;

    let result = repl::run_repl_with_prior(agent, &args, persistence, prior_messages).await;

    if let Some(ref handle) = maybe_handle
        && let Err(e) = ctx.runner.stop(handle).await
    {
        tracing::warn!("failed to stop llama-server after agent chat: {e}");
    }

    result
}

/// Create a new conversation for a fresh session.
async fn new_conversation<'a>(
    ctx: &'a CliContext,
    args: &ChatArgs,
) -> (Option<Conversation<'a>>, Vec<AgentMessage>) {
    let settings = ConversationSettingsBuilder::new(&args.sampling, &args.context)
        .model_name(&args.identifier)
        .tools(args.tools.clone(), args.no_tools)
        .agent_params(args.max_iterations, args.tool_timeout_ms, args.max_parallel)
        .build();

    let persistence = match Conversation::create(
        ctx.app.chat_history(),
        args.system_prompt.clone(),
        None,
        Some(settings),
    )
    .await
    {
        Ok(conv) => Some(conv),
        Err(e) => {
            tracing::warn!("failed to create agent conversation: {e}");
            None
        }
    };

    (persistence, Vec::new())
}

/// Load a previous conversation, merge its saved settings into args, and prepare for resume.
///
/// Settings restoration follows the principle: **saved settings are defaults,
/// explicit CLI flags override**. For example:
/// ```text
/// gglib chat other-model --continue 42 --temperature 0.9
/// ```
/// uses `other-model` and temperature `0.9` from the CLI, but restores
/// everything else (system prompt, top_p, tools, etc.) from conversation 42.
async fn resume_conversation<'a>(
    ctx: &'a CliContext,
    args: &ChatArgs,
    conv_id: i64,
) -> Result<(ChatArgs, Conversation<'a>, Vec<AgentMessage>)> {
    let history = ctx.app.chat_history();

    let conv = history
        .get_conversation(conv_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("conversation {conv_id} not found"))?;

    let db_messages = history.get_messages(conv_id).await?;
    let msg_count = db_messages.len();

    if msg_count == 0 {
        println!("Conversation #{conv_id} has no messages — starting fresh.");
    } else {
        print_memory_jogger(&db_messages, &conv.title);
    }

    // Merge saved settings into a copy of the current args.
    let merged = apply_saved_settings(args, &conv.system_prompt, &conv.settings);

    if merged.identifier.is_empty() {
        bail!(
            "cannot resume conversation #{conv_id}: no model name was saved and none was provided on the CLI"
        );
    }

    // Convert persisted messages to agent messages
    let prior_messages: Vec<AgentMessage> =
        db_messages.iter().map(|m| m.to_agent_message()).collect();

    let persistence = Conversation::resume(history, conv_id, msg_count).await;

    Ok((merged, persistence, prior_messages))
}

/// Merge saved [`ConversationSettings`] into [`ChatArgs`].
///
/// CLI-provided values always win; saved settings fill in blanks.
fn apply_saved_settings(
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
    if let Some(saved_max) = saved.max_iterations {
        // 25 is the clap default — treat it as "not set by user".
        if merged.max_iterations == 25 {
            merged.max_iterations = saved_max;
        }
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
fn print_memory_jogger(db_messages: &[gglib_core::domain::chat::Message], title: &str) {
    use gglib_core::domain::chat::MessageRole;

    println!("\n{}Resuming: {}{}\n", style::INFO, title, style::RESET,);

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
