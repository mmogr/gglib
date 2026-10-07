//! Single-turn agentic question handler for `gglib q`.
//!
//! Composes an agent loop with filesystem tools sandboxed to the current
//! working directory, sends a single user message, drains the event stream,
//! and optionally transitions into an interactive REPL session if the user
//! wants to continue the conversation.

use std::env;
use std::io::{self, IsTerminal};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage, TurnLimits};

use crate::bootstrap::CliContext;
use crate::conversation_settings::ConversationSettingsBuilder;
use crate::handlers::agent_chat::config::{AgentSessionParams, compose};
use crate::handlers::agent_chat::drain::drain_event_stream;
use crate::handlers::agent_chat::images::TurnImages;
use crate::handlers::agent_chat::persistence::{Conversation, Reply};
use crate::handlers::agent_chat::repl::run_repl_with_history;
use crate::handlers::agent_chat::sight::Sight;
use crate::shared_args::{ContextArgs, SamplingArgs};
use crate::target::Target;

/// System prompt for the agentic question mode.
const SYSTEM_PROMPT: &str = "\
You are an expert code analyst. You have access to filesystem tools \
(read_file, list_directory, grep_search) scoped to the user's working \
directory. Use them to explore the codebase and answer the question \
thoroughly. Be direct and concise.";

/// Arguments for the question command.
///
/// A bag rather than a parameter list, matching how `chat` already passes
/// [`ChatArgs`](super::chat::ChatArgs): fifteen positional arguments made
/// every call site a counting exercise and needed
/// `#[allow(clippy::too_many_arguments)]` to compile clean.
pub(crate) struct QuestionArgs {
    pub question: String,
    pub model_arg: Option<String>,
    pub file: Option<String>,
    pub port: Option<u16>,
    /// Which machine the turn runs on (ADR 0013).
    pub target: Target,
    pub max_iterations: Option<usize>,
    pub tools: Vec<String>,
    pub tool_timeout_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    /// `--image`: the files attached to the question.
    pub images: Vec<std::path::PathBuf>,
    pub observation_tools: Vec<String>,
    pub max_observation_steps: Option<usize>,
    /// `--show-prompt`: echo the assembled user message before sending.
    pub show_prompt: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub sampling: SamplingArgs,
    /// Named sampling profile, the flag form of a `{model}:{profile}` suffix.
    pub profile: Option<String>,
    pub context: ContextArgs,
}

/// Run a single-turn agentic question, with optional continuation into chat.
#[allow(
    clippy::default_trait_access,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn execute(ctx: &CliContext, args: QuestionArgs) -> Result<()> {
    let QuestionArgs {
        question,
        model_arg,
        file,
        port,
        target,
        max_iterations,
        tools,
        tool_timeout_ms,
        max_parallel,
        images,
        observation_tools,
        max_observation_steps,
        show_prompt,
        verbose,
        quiet,
        sampling,
        profile,
        context,
    } = args;
    // A file that cannot be attached ends the command before any model lookup.
    let (attachments, mut receipts) = (ctx.app.attachments(), io::stderr());
    let mut images = TurnImages::attach(attachments, &images, quiet, &mut receipts).await?;
    let cwd = env::current_dir().map_err(|e| anyhow!("cannot determine CWD: {e}"))?;

    let params = AgentSessionParams {
        model_identifier: model_arg.clone().unwrap_or_default(),
        ctx_size: context.ctx_size,
        port,
        target,
        tools: tools.clone(),
        model_name: model_arg.clone(),
        // `gglib q` takes no retry flag; the environment defaults apply.
        retry_policy: gglib_core::retry::RetryPolicy::from_env(),
        // Filled in below, once settings have supplied the profile list.
        profile: None,
        turn: None,
    };

    // If no model was specified, look up the default from settings
    let settings = ctx
        .app
        .settings()
        .get()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?;

    // Resolve `--profile` or a `{model}:{profile}` suffix before anything asks
    // the daemon to start `model_identifier` — the suffix must not reach lookup.
    // What that means on the paired machine, whose profiles these are not, is
    // `profile_selection`'s to say.
    let selection = super::profile_selection::select_for_upstream(
        target,
        ctx.catalog.as_ref(),
        settings.inference_profiles.as_deref().unwrap_or_default(),
        &params.model_identifier,
        profile.as_deref(),
    )
    .await?;
    // A turn with no model named: this machine's default here, by id, the
    // model last asked for there — then resolved once, on that machine.
    let default_id = settings.default_model_id;
    let model_identifier = target
        .model_for_turn(ctx, selection.model, async || {
            let default_id = default_id.ok_or_else(|| {
                anyhow!(
                    "No model specified and no default model set.\n\
                     Use --model <id-or-name> or set a default:\n  \
                     gglib config default <id-or-name>"
                )
            })?;
            let model = ctx
                .app
                .models()
                .get_by_id(default_id)
                .await
                .map_err(|e| anyhow!("failed to load default model: {e}"))?
                .ok_or_else(|| anyhow!("default model (ID: {default_id}) not found"))?;
            Ok(model.id.to_string())
        })
        .await?;
    let turn = target.resolve_turn(ctx, model_identifier).await?;
    // `model_name` is what goes in the request body: the stripped name here,
    // where a `{model}:{profile}` suffix would name no model, and there the
    // id the far machine resolved, which carries its profile.
    let model_name = params.model_name.as_ref().map(|_| turn.identifier.clone());
    let mut reply = Reply::new(turn.made_by());
    let params = AgentSessionParams {
        model_name: target.wire_model_name(model_name, &turn.identifier),
        model_identifier: turn.identifier.clone(),
        profile: selection.profile,
        turn: Some(turn),
        ..params
    };

    let inference_config = sampling.into_inference_config();
    let sampling_override = if inference_config == Default::default() {
        None
    } else {
        Some(inference_config)
    };

    let sight = Sight::of_session(ctx, &params).await;
    images.judge(sight, &[]).await?;
    let agent = compose(
        ctx,
        &params,
        Some(cwd.clone()),
        sampling_override.clone(),
        &crate::handlers::agent_chat::config::BannerInfo {
            quiet,
            sampling: sampling_override,
            prior_history_chars: None,
        },
    )
    .await?;

    // The flag, then this machine's stored limits, as the daemon resolves a
    // turn's.
    let limits = TurnLimits::resolve(max_iterations, Some(&settings));

    let config = AgentConfig::from_user_params(
        Some(limits.max_iterations),
        max_parallel,
        tool_timeout_ms,
        // Some(vec) replaces defaults; empty vec passes None to preserve defaults.
        Some(observation_tools).filter(|v| !v.is_empty()),
        max_observation_steps,
        limits.max_stagnation_steps,
    )
    .map_err(|e| anyhow!("invalid agent config: {e}"))?;

    // Build messages
    let system_prompt = format!("{}\n\nWorking directory: {}", SYSTEM_PROMPT, cwd.display());
    let mut messages = vec![AgentMessage::System {
        content: system_prompt.clone(),
    }];

    // Construct user message with optional piped/file context
    let user_content =
        super::question_input::build_user_message(&question, file.as_deref(), show_prompt)?;
    messages.push(AgentMessage::User {
        content: user_content,
        images: images.take(),
    });
    let asked = messages.last().cloned();

    // Run the agent loop
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
    let agent_clone = Arc::clone(&agent);
    let messages_for_task = messages;
    let config_clone = config.clone();
    let handle = tokio::spawn(async move {
        match agent_clone.run(messages_for_task, config_clone, tx).await {
            Ok(output) => Some(output.history),
            Err(e) => {
                tracing::debug!("agent loop ended: {e}");
                None
            }
        }
    });

    // Drain events with Ctrl+C support
    let completed = tokio::select! {
        biased;
        result = drain_event_stream(&mut rx, verbose, quiet, Some(&mut reply)) => result,
        _ = tokio::signal::ctrl_c() => {
            handle.abort();
            while rx.try_recv().is_ok() {}
            eprintln!("\n[cancelled — Ctrl+C]");
            false
        }
    };

    let history = handle.await.ok().flatten();

    // ── Persist conversation ─────────────────────────────────────────────
    // Save the question and the reply as it arrived to the DB so they appear
    // in the GUI conversation list and can later be resumed.  Best-effort: a
    // persistence failure must never break the interactive session.
    let mut persistence = None;
    if completed
        && history.is_some()
        && let Some(turn) = &params.turn
    {
        let settings =
            ConversationSettingsBuilder::new(&SamplingArgs::default(), &ContextArgs::default())
                .turn(turn)
                .tools(tools.clone(), false)
                .agent_params(max_iterations, tool_timeout_ms, max_parallel)
                .build();
        let (chats, made_by) = (ctx.app.chat_history(), turn.made_by());
        match Conversation::create(chats, Some(system_prompt), Some(settings), made_by).await {
            Ok(conv) => {
                conv.save_user(asked.as_ref()).await;
                conv.save_reply(&reply, true).await;
                persistence = Some(conv);
            }
            Err(e) => tracing::warn!("failed to create agent conversation: {e}"),
        }
    }

    // ── Continuation prompt ──────────────────────────────────────────────
    // Offer to continue chatting if the initial question succeeded and we
    // are in an interactive terminal.  Skip when:
    //   - quiet mode (-Q) — script-friendly output
    //   - stdin is not a TTY (piped input) — would read garbage or hang
    //   - the agent didn't produce a usable history
    let interactive = !quiet && io::stdin().is_terminal();

    if completed && interactive {
        if let Some(history) = history
            && super::question_input::ask_continue()?
        {
            run_repl_with_history(agent, history, config, verbose, persistence, images).await?;
        }
    } else if !completed {
        return Err(anyhow!("agent did not produce a final answer"));
    }

    // The llama-server belongs to the daemon and stays warm for the next
    // session; nothing to stop here.
    Ok(())
}
