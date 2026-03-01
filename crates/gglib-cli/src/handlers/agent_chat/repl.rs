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

use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_core::ports::AgentLoopPort;
use gglib_core::AGENT_EVENT_CHANNEL_CAPACITY;

use crate::handlers::chat::ChatArgs;

use super::renderer::render_event;

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
pub async fn run_repl(agent_loop: Arc<dyn AgentLoopPort>, args: &ChatArgs) -> Result<()> {
    let mut config = AgentConfig::default();
    config.max_iterations = args.max_iterations;
    if let Some(ms) = args.tool_timeout_ms { config.tool_timeout_ms = ms; }
    if let Some(n) = args.max_parallel { config.max_parallel_tools = n; }

    // Wrap the editor in Arc<Mutex> so it can be moved into spawn_blocking
    // on each turn while retaining readline history across turns.
    let editor: Arc<Mutex<DefaultEditor>> =
        Arc::new(Mutex::new(DefaultEditor::new().map_err(|e| {
            anyhow::anyhow!("failed to initialise readline editor: {e}")
        })?));

    // Conversation history shared across turns.
    let mut messages: Vec<AgentMessage> = Vec::new();
    if let Some(ref system) = args.system_prompt {
        messages.push(AgentMessage::System {
            content: system.clone(),
        });
    }

    println!("Agentic chat ready. Type /help for help, /quit to exit.");

    // ── REPL outer loop ──────────────────────────────────────────────────────
    loop {
        // ── 1. Read user input (blocking → spawn_blocking) ───────────────────
        let ed = Arc::clone(&editor);
        let line = tokio::task::spawn_blocking(move || {
            ed.lock().expect("editor poisoned").readline("You: ")
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

        messages.push(AgentMessage::User {
            content: input.clone(),
        });

        // ── 2. Run agent loop for this turn ──────────────────────────────────
        let (tx, mut rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

        let agent = Arc::clone(&agent_loop);
        let msgs = messages.clone();
        let cfg = config.clone();

        let handle: JoinHandle<Option<Vec<AgentMessage>>> = tokio::spawn(async move {
            match agent.run(msgs, cfg, tx).await {
                Ok(output) => Some(output.history),
                Err(e) => {
                    tracing::debug!("agent loop ended: {e}");
                    None
                }
            }
        });

        // ── 3. Consume event stream; Ctrl+C aborts the agent task ────────────────
        let completed = tokio::select! {
            biased;
            result = drain_event_stream(&mut rx, args.verbose) => result,
            _ = tokio::signal::ctrl_c() => {
                handle.abort();
                while rx.try_recv().is_ok() {}
                eprintln!("\n[agent response cancelled — Ctrl+C]");
                false
            }
        };

        // ── 4. Replace conversation history with the loop’s accumulated messages.
        //
        // The loop appends every assistant + tool-result message during the run
        // and includes the final assistant reply, so `new_messages` is the
        // complete context needed for the next turn.
        //
        // On Ctrl+C (`completed = false`) or loop error (handle returns `None`)
        // the history stays unchanged — failed or cancelled turns are not added.
        if completed && let Ok(Some(new_messages)) = handle.await {
            messages = new_messages;
        }
    }

    Ok(())
}

// =============================================================================
// Private helpers
// =============================================================================

/// Drain `rx` until the channel closes or a [`AgentEvent::FinalAnswer`]
/// arrives, rendering each event.
///
/// Returns `true` only when the turn completed with a [`AgentEvent::FinalAnswer`]
/// event.  Returns `false` when the channel closes without one (e.g. the loop
/// hit max iterations or stagnated).  Cancellation (Ctrl+C) is handled by the
/// caller via `tokio::select!`; this function has no side effects beyond
/// rendering.
///
/// The caller **must** gate any history update on the return value: history
/// from a failed or incomplete turn must not replace the previous context.
async fn drain_event_stream(rx: &mut mpsc::Receiver<AgentEvent>, verbose: bool) -> bool {
    while let Some(event) = rx.recv().await {
        render_event(&event, verbose);

        if let AgentEvent::FinalAnswer { .. } = event {
            // `FinalAnswer` is always the last event emitted before the loop
            // drops its `Sender`.  Any events after this would be a protocol
            // violation and are intentionally dropped.
            return true;
        }
    }
    // Channel closed without a FinalAnswer — the loop ended with an error
    // (max iterations, stagnation, etc.).  The caller must not update history.
    false
}
