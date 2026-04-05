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

use anyhow::Result;

use gglib_core::domain::agent::AgentMessage;

use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;
use crate::presentation::style;
use crate::shared_args::ConversationSettingsBuilder;

use self::persistence::Conversation;

/// Entry point: start the interactive agentic REPL.
///
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
/// When `args.continue_id` is set, loads a previous conversation and resumes.
pub async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    let inference_config = args.sampling.clone().into_inference_config();
    let sampling = if inference_config == Default::default() {
        None
    } else {
        Some(inference_config)
    };
    let (agent, maybe_handle) = config::compose(ctx, &args.into(), None, sampling).await?;

    let (persistence, prior_messages) = if let Some(conv_id) = args.continue_id {
        resume_conversation(ctx, args, conv_id).await?
    } else {
        new_conversation(ctx, args).await
    };

    let result =
        repl::run_repl_with_prior(agent, args, persistence, prior_messages).await;

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

/// Load a previous conversation and prepare for resume.
async fn resume_conversation<'a>(
    ctx: &'a CliContext,
    _args: &ChatArgs,
    conv_id: i64,
) -> Result<(Option<Conversation<'a>>, Vec<AgentMessage>)> {
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
        // Memory jogger: show the last user+assistant exchange
        print_memory_jogger(&db_messages, &conv.title);
    }

    // Convert persisted messages to agent messages
    let prior_messages: Vec<AgentMessage> =
        db_messages.iter().map(|m| m.to_agent_message()).collect();

    let persistence = Conversation::resume(history, conv_id, msg_count).await;

    Ok((Some(persistence), prior_messages))
}

/// Print the last user/assistant exchange as a memory jogger when resuming.
fn print_memory_jogger(
    db_messages: &[gglib_core::domain::chat::Message],
    title: &str,
) {
    use gglib_core::domain::chat::MessageRole;

    println!(
        "\n{}Resuming: {}{}\n",
        style::INFO,
        title,
        style::RESET,
    );

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
        println!(
            "{}  Assistant: {}{}",
            style::DIM, content, style::RESET
        );
    }
    println!();
}
