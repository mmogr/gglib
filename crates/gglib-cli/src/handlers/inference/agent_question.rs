//! Single-turn agentic question handler for `gglib q`.
//!
//! Composes an agent loop with filesystem tools sandboxed to the current
//! working directory, sends a single user message, drains the event stream,
//! and optionally transitions into an interactive REPL session if the user
//! wants to continue the conversation.

use std::env;
use std::io::{self, IsTerminal, Write};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};

use crate::bootstrap::CliContext;
use crate::handlers::agent_chat::config::{AgentSessionParams, compose};
use crate::handlers::agent_chat::drain::drain_event_stream;
use crate::handlers::agent_chat::persistence::Conversation;
use crate::handlers::agent_chat::repl::run_repl_with_history;
use crate::handlers::inference::shared::resolve_max_iterations;
use crate::shared_args::{ContextArgs, SamplingArgs};

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
    pub max_iterations: Option<usize>,
    pub tools: Vec<String>,
    pub tool_timeout_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    pub observation_tools: Vec<String>,
    pub max_observation_steps: Option<usize>,
    pub verbose: bool,
    pub quiet: bool,
    pub sampling: SamplingArgs,
    /// Named sampling profile, the flag form of a `{model}:{profile}` suffix.
    pub profile: Option<String>,
    pub context: ContextArgs,
}

/// Run a single-turn agentic question, with optional continuation into chat.
pub(crate) async fn execute(ctx: &CliContext, args: QuestionArgs) -> Result<()> {
    let QuestionArgs {
        question,
        model_arg,
        file,
        port,
        max_iterations,
        tools,
        tool_timeout_ms,
        max_parallel,
        observation_tools,
        max_observation_steps,
        verbose,
        quiet,
        sampling,
        profile,
        context,
    } = args;
    let cwd = env::current_dir().map_err(|e| anyhow!("cannot determine CWD: {e}"))?;

    let params = AgentSessionParams {
        model_identifier: model_arg.clone().unwrap_or_default(),
        ctx_size: context.ctx_size,
        port,
        tools: tools.clone(),
        model_name: model_arg.clone(),
        // `gglib q` takes no retry flag; the environment defaults apply.
        retry_policy: gglib_core::retry::RetryPolicy::from_env(),
        // Filled in below, once settings have supplied the profile list.
        profile: None,
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
    let selection = super::profile_selection::select(
        ctx.catalog.as_ref(),
        settings.inference_profiles.as_deref().unwrap_or_default(),
        &params.model_identifier,
        profile.as_deref(),
    )
    .await?;
    let params = AgentSessionParams {
        // Both, from the stripped name: `model_name` is what goes in the
        // request body, and a `{model}:{profile}` suffix there would ask the
        // upstream for a model that does not exist.
        model_name: params.model_name.as_ref().map(|_| selection.model.clone()),
        model_identifier: selection.model,
        profile: selection.profile,
        ..params
    };

    let params = if params.model_identifier.is_empty() {
        let default_id = settings.default_model_id.ok_or_else(|| {
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
        AgentSessionParams {
            model_identifier: model.name.clone(),
            ..params
        }
    } else {
        params
    };

    let inference_config = sampling.into_inference_config();
    let sampling_override = if inference_config == Default::default() {
        None
    } else {
        Some(inference_config)
    };

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

    let resolved_max_iterations = resolve_max_iterations(max_iterations, &settings);

    let config = AgentConfig::from_user_params(
        Some(resolved_max_iterations),
        max_parallel,
        tool_timeout_ms,
        // Some(vec) replaces defaults; empty vec passes None to preserve defaults.
        Some(observation_tools).filter(|v| !v.is_empty()),
        max_observation_steps,
        settings.max_stagnation_steps.map(|v| v as usize),
    )
    .map_err(|e| anyhow!("invalid agent config: {e}"))?;

    // Build messages
    let mut messages = vec![AgentMessage::System {
        content: format!("{}\n\nWorking directory: {}", SYSTEM_PROMPT, cwd.display()),
    }];

    // Construct user message with optional piped/file context
    let user_content =
        super::question_input::build_user_message(&question, file.as_deref(), verbose)?;
    messages.push(AgentMessage::User {
        content: user_content,
    });

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
        result = drain_event_stream(&mut rx, verbose, quiet) => result,
        _ = tokio::signal::ctrl_c() => {
            handle.abort();
            while rx.try_recv().is_ok() {}
            eprintln!("\n[cancelled — Ctrl+C]");
            false
        }
    };

    let history = handle.await.ok().flatten();

    // ── Persist conversation ─────────────────────────────────────────────
    // Save the full agent exchange to the DB so it appears in the GUI
    // conversation list and can later be resumed.  Best-effort: a
    // persistence failure must never break the interactive session.
    let mut persistence = None;
    if completed && let Some(ref history) = history {
        let system_prompt = format!("{}\n\nWorking directory: {}", SYSTEM_PROMPT, cwd.display());
        let settings = crate::conversation_settings::ConversationSettingsBuilder::new(
            &SamplingArgs::default(),
            &crate::shared_args::ContextArgs::default(),
        )
        .model_name(params.model_identifier.clone())
        .tools(tools.clone(), false)
        .agent_params(max_iterations, tool_timeout_ms, max_parallel)
        .build();
        match Conversation::create(
            ctx.app.chat_history(),
            Some(system_prompt),
            None,
            Some(settings),
        )
        .await
        {
            Ok(mut conv) => {
                conv.save_new(history).await;
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
            && ask_continue()?
        {
            run_repl_with_history(agent, history, config, verbose, persistence).await?;
        }
    } else if !completed {
        return Err(anyhow!("agent did not produce a final answer"));
    }

    // The llama-server belongs to the daemon and stays warm for the next
    // session; nothing to stop here.
    Ok(())
}

/// Prompt the user to continue into an interactive chat session.
///
/// Returns `true` for 'y', 'Y', or empty input (Enter); `false` for
/// anything else.  EOF (Ctrl+D) is treated as a clean decline.
fn ask_continue() -> Result<bool> {
    // Flush stdout to ensure the agent's final output is fully rendered
    // before we print the prompt — prevents interleaving.
    io::stdout().flush().ok();
    eprintln!();
    eprint!("[Continue chatting? (y/n)] ");
    io::stderr().flush().ok();

    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|e| anyhow!("failed to read input: {e}"))?;

    // EOF (Ctrl+D) → treat as 'n'
    if bytes == 0 {
        eprintln!();
        return Ok(false);
    }

    let answer = input.trim();
    Ok(answer.is_empty() || answer.eq_ignore_ascii_case("y"))
}
