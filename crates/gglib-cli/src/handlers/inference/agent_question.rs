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
use crate::shared_args::SamplingArgs;

/// System prompt for the agentic question mode.
const SYSTEM_PROMPT: &str = "\
You are an expert code analyst. You have access to filesystem tools \
(read_file, list_directory, grep_search) scoped to the user's working \
directory. Use them to explore the codebase and answer the question \
thoroughly. Be direct and concise.";

/// Run a single-turn agentic question, with optional continuation into chat.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    ctx: &CliContext,
    question: String,
    model_arg: Option<String>,
    file: Option<String>,
    port: Option<u16>,
    max_iterations: usize,
    tools: Vec<String>,
    tool_timeout_ms: Option<u64>,
    max_parallel: Option<usize>,
    verbose: bool,
    quiet: bool,
    sampling: SamplingArgs,
) -> Result<()> {
    let cwd = env::current_dir().map_err(|e| anyhow!("cannot determine CWD: {e}"))?;

    let params = AgentSessionParams {
        model_identifier: model_arg.clone().unwrap_or_default(),
        ctx_size: None,
        port,
        tools: tools.clone(),
        model_name: model_arg.clone(),
    };

    // If no model was specified, look up the default from settings
    let params = if params.model_identifier.is_empty() {
        let settings = ctx
            .app
            .settings()
            .get()
            .await
            .map_err(|e| anyhow!("failed to load settings: {e}"))?;
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

    let (agent, maybe_handle) = compose(ctx, &params, Some(cwd.clone()), sampling_override).await?;

    let config = AgentConfig::from_user_params(Some(max_iterations), max_parallel, tool_timeout_ms)
        .map_err(|e| anyhow!("invalid agent config: {e}"))?;

    // Build messages
    let mut messages = vec![AgentMessage::System {
        content: format!("{}\n\nWorking directory: {}", SYSTEM_PROMPT, cwd.display()),
    }];

    // Construct user message with optional piped/file context
    let user_content = build_user_message(&question, file.as_deref(), verbose)?;
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
        let settings = crate::shared_args::ConversationSettingsBuilder::new(
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
        // Defer error until after potential server cleanup
        stop_server(ctx, &maybe_handle).await;
        return Err(anyhow!("agent did not produce a final answer"));
    }

    stop_server(ctx, &maybe_handle).await;
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

/// Stop the auto-started llama-server, if any.
async fn stop_server(ctx: &CliContext, maybe_handle: &Option<gglib_core::ProcessHandle>) {
    if let Some(server_handle) = maybe_handle
        && let Err(e) = ctx.runner.stop(server_handle).await
    {
        tracing::warn!("failed to stop llama-server: {e}");
    }
}

/// Build the user message, incorporating piped stdin or `--file` content.
fn build_user_message(question: &str, file: Option<&str>, verbose: bool) -> Result<String> {
    use std::io::{self, IsTerminal, Read};

    // --file takes precedence over piped stdin.
    let context = if let Some(path) = file {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("failed to read file '{}': {e}", path))?;
        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    } else {
        let stdin = io::stdin();
        if !stdin.is_terminal() {
            let mut buffer = String::new();
            stdin
                .lock()
                .read_to_string(&mut buffer)
                .map_err(|e| anyhow!("failed to read from stdin: {e}"))?;
            if buffer.is_empty() {
                None
            } else {
                Some(buffer)
            }
        } else {
            None
        }
    };

    let user_message = match context {
        Some(input) => {
            if question.contains("{}") {
                question.replace("{}", &input)
            } else {
                format!("<context>\n{}\n</context>\n\n{}", input.trim(), question)
            }
        }
        None => question.to_string(),
    };

    if verbose {
        eprintln!("─── User Message ───");
        eprintln!("{user_message}");
        eprintln!("─── End ───\n");
    }

    Ok(user_message)
}
