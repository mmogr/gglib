//! Single-turn agentic question handler for `gglib q --agent`.
//!
//! Composes an agent loop with filesystem tools sandboxed to the current
//! working directory, sends a single user message, drains the event stream,
//! and exits.

use std::env;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};

use crate::bootstrap::CliContext;
use crate::handlers::agent_chat::config::{AgentSessionParams, compose};
use crate::handlers::agent_chat::renderer::drain_event_stream;
use crate::handlers::question::QuestionArgs;

/// System prompt for the agentic question mode.
const SYSTEM_PROMPT: &str = "\
You are an expert code analyst. You have access to filesystem tools \
(read_file, list_directory, grep_search) scoped to the user's working \
directory. Use them to explore the codebase and answer the question \
thoroughly. Be direct and concise.";

/// Run a single-turn agentic question.
pub async fn execute(ctx: &CliContext, args: &QuestionArgs) -> Result<()> {
    let cwd = env::current_dir().map_err(|e| anyhow!("cannot determine CWD: {e}"))?;

    let params = AgentSessionParams {
        model_identifier: args.model.clone().unwrap_or_default(),
        ctx_size: args.ctx_size.clone(),
        port: args.port,
        tools: args.tools.clone(),
        model_name: args.model.clone(),
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

    let (agent, maybe_handle) = compose(ctx, &params, Some(cwd.clone())).await?;

    let config = AgentConfig::from_user_params(
        Some(args.max_iterations),
        args.max_parallel,
        args.tool_timeout_ms,
    )
    .map_err(|e| anyhow!("invalid agent config: {e}"))?;

    // Build messages
    let mut messages = vec![AgentMessage::System {
        content: format!("{}\n\nWorking directory: {}", SYSTEM_PROMPT, cwd.display()),
    }];

    // Construct user message with optional piped context
    let user_content = build_user_message(&args.question, args.verbose)?;
    messages.push(AgentMessage::User {
        content: user_content,
    });

    // Run the agent loop
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
    let agent_clone = Arc::clone(&agent);
    let handle = tokio::spawn(async move { agent_clone.run(messages, config, tx).await });

    // Drain events with Ctrl+C support
    let completed = tokio::select! {
        biased;
        result = drain_event_stream(&mut rx, args.verbose) => result,
        _ = tokio::signal::ctrl_c() => {
            handle.abort();
            while rx.try_recv().is_ok() {}
            eprintln!("\n[cancelled — Ctrl+C]");
            false
        }
    };

    let _ = handle.await;

    // Stop auto-started server
    if let Some(ref server_handle) = maybe_handle
        && let Err(e) = ctx.runner.stop(server_handle).await
    {
        tracing::warn!("failed to stop llama-server: {e}");
    }

    if !completed {
        return Err(anyhow!("agent did not produce a final answer"));
    }

    Ok(())
}

/// Build the user message, incorporating piped stdin if available.
fn build_user_message(question: &str, verbose: bool) -> Result<String> {
    use std::io::{self, IsTerminal, Read};

    let stdin = io::stdin();
    let piped_input = if !stdin.is_terminal() {
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
    };

    let user_message = match piped_input {
        Some(input) => {
            if question.contains("{}") {
                question.replace("{}", &input)
            } else {
                format!(
                    "<piped_input>\n{}\n</piped_input>\n\n{}",
                    input.trim(),
                    question
                )
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
