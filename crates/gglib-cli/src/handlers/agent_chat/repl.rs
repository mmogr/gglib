//! Async REPL loop for `gglib chat --agent`.
//!
//! # Design choices
//!
//! ## Blocking I/O and the Tokio runtime
//!
//! `rustyline::DefaultEditor::readline()` is synchronous, blocking the calling
//! thread until the user presses Enter.  Calling a blocking function directly
//! on a Tokio worker thread prevents other futures (MCP connections, SSE
//! streams, health checks) from running.
//!
//! **Solution**: each call to `readline` is wrapped in
//! [`tokio::task::spawn_blocking`], which hands the call to a dedicated
//! blocking-thread pool, returning the async executor immediately.
//!
//! ## Ctrl+C cancellation
//!
//! A Ctrl+C **during an agent response** is handled via a `tokio::select!` in
//! `run_repl`: the agent-loop task is aborted, the event channel is drained,
//! and the REPL returns to the prompt — the `handle.abort()` side effect is
//! explicit at the call site, not hidden inside `collect_events`.
//!
//! A Ctrl+C **at the readline prompt** is signalled by rustyline as
//! [`ReadlineError::Interrupted`]; the REPL prints a hint and continues to
//! the next prompt instead of exiting (keeping behaviour consistent with the
//! help text — `/quit` or Ctrl+D are the intended exit paths).

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_core::ports::AgentLoopPort;

use crate::handlers::inference::chat::ChatArgs;

use super::drain::drain_event_stream;
use super::persistence::Conversation;

// =============================================================================
// Help text
// =============================================================================

const REPL_HELP: &str = "\
  /help     print this message
  /quit     exit the session
  /exit     exit the session
  Ctrl+C    cancel the current agent response (return to prompt)
  Ctrl+D    exit the session (EOF)";

// =============================================================================
// Public entry point
// =============================================================================

/// Run the interactive agent REPL until the user quits or reaches EOF.
///
/// Takes the agent loop as `Arc<dyn AgentLoopPort>` so the REPL can cheaply
/// clone the reference for each spawned per-turn task without requiring
/// [`AgentLoop`] to implement [`Clone`].
pub async fn run_repl(
    agent_loop: Arc<dyn AgentLoopPort>,
    args: &ChatArgs,
    persistence: Option<Conversation<'_>>,
) -> Result<()> {
    let config = AgentConfig::from_user_params(
        Some(args.max_iterations),
        args.max_parallel,
        args.tool_timeout_ms,
    )
    .map_err(|e| anyhow::anyhow!("invalid agent config: {e}"))?;

    // Conversation history shared across turns.
    let mut messages: Vec<AgentMessage> = Vec::new();
    if let Some(ref system) = args.system_prompt {
        messages.push(AgentMessage::System {
            content: system.clone(),
        });
    }

    run_repl_with_history(agent_loop, messages, config, args.verbose, persistence).await
}

/// Run the interactive agent REPL with a pre-populated conversation history.
///
/// This is the core REPL implementation.  [`run_repl`] delegates here after
/// building the initial message list and config from [`ChatArgs`].  It is
/// also called directly by `gglib q --agent` to transition from a single-turn
/// question into an interactive session, carrying the full conversation
/// history forward.
pub async fn run_repl_with_history(
    agent_loop: Arc<dyn AgentLoopPort>,
    mut messages: Vec<AgentMessage>,
    config: AgentConfig,
    verbose: bool,
    mut persistence: Option<Conversation<'_>>,
) -> Result<()> {
    // Wrap the editor in Arc<Mutex> so it can be moved into spawn_blocking
    // on each turn while retaining readline history across turns.
    let editor: Arc<Mutex<DefaultEditor>> =
        Arc::new(Mutex::new(DefaultEditor::new().map_err(|e| {
            anyhow::anyhow!("failed to initialise readline editor: {e}")
        })?));

    println!("Agentic chat ready. Type /help for help, /quit to exit.");

    // ── REPL outer loop ──────────────────────────────────────────────────────
    loop {
        // ── 1. Read user input (blocking → spawn_blocking) ───────────────────
        let ed = Arc::clone(&editor);
        let line = tokio::task::spawn_blocking(move || {
            ed.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .readline("You: ")
        })
        .await?;

        let input = match line {
            Ok(text) => text,
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C at the prompt → cancel any pending input and return
                // to the prompt.  (Use /quit or Ctrl+D to exit the session.)
                println!("[use /quit or Ctrl+D to exit]");
                continue;
            }
            Err(ReadlineError::Eof) => break, // Ctrl+D / EOF
            Err(e) => return Err(anyhow::anyhow!("readline error: {e}")),
        };

        let input = input.trim().to_owned();

        match input.as_str() {
            "" => continue,
            "/quit" | "/exit" => break,
            "/help" => {
                println!("{REPL_HELP}");
                continue;
            }
            _ => {}
        }

        messages.push(AgentMessage::User { content: input });

        // ── 2–4. Run turn and update history ─────────
        // Context pruning is handled by the agent loop itself (`prune_for_budget`
        // is called before the first LLM call and after each tool-execution
        // iteration).  The returned `output.history` is already within budget,
        // so a redundant prune here is unnecessary.
        //
        // Move `messages` into the turn; the function moves it into the
        // spawned task and returns either the updated history (success) or
        // the original snapshot (failure / Ctrl+C) — one clone total instead
        // of the previous two.
        messages = run_single_turn(&agent_loop, messages, config.clone(), verbose).await;

        // Persist new messages (best-effort).
        if let Some(ref mut conv) = persistence {
            conv.save_new(&messages).await;
        }
    }

    Ok(())
}

// =============================================================================
// Private helpers
// =============================================================================

/// Run one agent turn: spawn the loop task, consume events, handle Ctrl+C,
/// and return the updated conversation history.
///
/// Takes ownership of `messages` to avoid a redundant clone at the call site.
/// A single clone is made internally as a backup (`pre_turn`); the original
/// is moved into the spawned task.
///
/// On success, returns the full `output.history` from the agent loop (which
/// includes assistant + tool-result messages appended during the turn).
///
/// On cancellation or agent error, returns the `pre_turn` snapshot so the
/// user's prior conversation context is preserved.
async fn run_single_turn(
    agent_loop: &Arc<dyn AgentLoopPort>,
    messages: Vec<AgentMessage>,
    config: AgentConfig,
    verbose: bool,
) -> Vec<AgentMessage> {
    // Single clone: keep a backup so a failed or cancelled turn restores the
    // exact conversation state (including the user message that triggered
    // this turn).  The original `messages` is moved into the spawned task.
    let pre_turn = messages.clone();

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
    let agent = Arc::clone(agent_loop);
    let handle: JoinHandle<Option<Vec<AgentMessage>>> = tokio::spawn(async move {
        match agent.run(messages, config, tx).await {
            Ok(output) => Some(output.history),
            Err(e) => {
                tracing::debug!("agent loop ended: {e}");
                None
            }
        }
    });

    let completed = tokio::select! {
        biased;
        result = drain_event_stream(&mut rx, verbose, false) => result,
        _ = tokio::signal::ctrl_c() => {
            handle.abort();
            while rx.try_recv().is_ok() {}
            eprintln!("\n[agent response cancelled — Ctrl+C]");
            false
        }
    };

    // Always await the handle — even after abort() — so any panic is not
    // silently dropped and the task is fully cleaned up.
    let loop_result = handle.await;

    if completed && let Ok(Some(new_messages)) = loop_result {
        return new_messages;
    }
    pre_turn
}
