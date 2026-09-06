#![doc = include_str!("README.md")]
pub(crate) mod config;
pub(crate) mod drain;
mod markdown;
pub(crate) mod persistence;
pub(crate) mod renderer;
pub(crate) mod repl;
pub(crate) mod resume_settings;
pub(crate) mod sampling_warning;
mod thinking_dispatch;
mod tool_format;
pub(crate) mod upstream;

use anyhow::{Result, bail};

use gglib_core::domain::agent::AgentMessage;

use crate::bootstrap::CliContext;
use crate::conversation_settings::ConversationSettingsBuilder;
use crate::handlers::inference::chat::ChatArgs;

use self::persistence::Conversation;

/// Entry point: start the interactive agentic REPL.
///
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
/// When `args.continue_id` is set, loads a previous conversation and resumes
/// with the original session parameters (saved settings fill in any CLI args
/// the user didn't explicitly provide).
pub(crate) async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    // 1. If resuming, load the conversation first and merge saved settings
    //    into args so the agent is composed with the correct parameters.
    let mut args = args.clone();

    // Resolve max_iterations and max_stagnation_steps from persisted settings
    // when not already provided (there is no per-run stagnation flag).
    if let Ok(settings) = ctx.app.settings().get().await {
        if args.max_iterations.is_none() {
            args.max_iterations = settings.max_tool_iterations.map(|v| v as usize);
        }
        if args.max_stagnation_steps.is_none() {
            args.max_stagnation_steps = settings.max_stagnation_steps.map(|v| v as usize);
        }
    }

    // Strip any `{model}:{profile}` suffix before a conversation is created:
    // the identifier is persisted, and a stored suffix would come back on
    // every resume as a profile the user did not type this time — colliding
    // with their `--profile` and making the session unresumable.
    let profile_settings = ctx.app.settings().get().await?;
    let configured_profiles = profile_settings
        .inference_profiles
        .as_deref()
        .unwrap_or_default();
    let typed_this_invocation = !args.identifier.is_empty();
    let mut selected_profile = None;
    if typed_this_invocation {
        let selection = crate::handlers::inference::profile_selection::select(
            ctx.catalog.as_ref(),
            configured_profiles,
            &args.identifier,
            args.profile.as_deref(),
        )
        .await?;
        args.identifier = selection.model;
        selected_profile = selection.profile;
    }

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
    let prior_chars: usize = prior_messages.iter().map(|m| m.char_count()).sum();
    let banner = config::BannerInfo {
        quiet: false,
        sampling: sampling.clone(),
        prior_history_chars: if prior_chars > 0 {
            Some(prior_chars)
        } else {
            None
        },
    };
    // On a resume the identifier came from storage, not from this command
    // line. An explicit `--profile` is therefore the only thing the user
    // actually typed, and it wins over any suffix an older conversation
    // recorded rather than colliding with it.
    if !typed_this_invocation {
        selected_profile = crate::handlers::inference::profile_selection::resume_profile(
            ctx.catalog.as_ref(),
            configured_profiles,
            &mut args.identifier,
            args.profile.as_deref(),
        )
        .await?;
    }

    let params = config::AgentSessionParams {
        model_identifier: args.identifier.clone(),
        profile: selected_profile,
        ..config::AgentSessionParams::from(&args)
    };
    let agent = config::compose(ctx, &params, None, sampling, &banner).await?;

    // The llama-server belongs to the daemon and stays warm for the next
    // session; nothing to stop here.
    repl::run_repl_with_prior(agent, &args, persistence, prior_messages).await
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
        resume_settings::print_memory_jogger(&db_messages, &conv.title);
    }

    // Merge saved settings into a copy of the current args.
    let merged = resume_settings::apply_saved_settings(args, &conv.system_prompt, &conv.settings);

    if merged.identifier.is_empty() {
        bail!(
            "cannot resume conversation #{conv_id}: no model name was saved and none was provided on the CLI"
        );
    }

    // Convert persisted messages to agent messages
    let mut prior_messages: Vec<AgentMessage> =
        db_messages.iter().map(|m| m.to_agent_message()).collect();

    // The system prompt is stored on the conversation record (not as a
    // message row), so prepend it if present.
    if let Some(ref prompt) = merged.system_prompt {
        prior_messages.insert(
            0,
            AgentMessage::System {
                content: prompt.clone(),
            },
        );
    }

    let persistence = Conversation::resume(history, conv_id, msg_count).await;

    Ok((merged, persistence, prior_messages))
}
