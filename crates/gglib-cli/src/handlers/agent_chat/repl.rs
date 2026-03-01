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
//! `tokio::signal::ctrl_c()` is used inside a `tokio::select!` to abort the
//! running agent-loop task without terminating the process.  A Ctrl+C at the
//! readline prompt is handled by rustyline itself, which returns
//! [`ReadlineError::Interrupted`].

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use gglib_core::domain::agent::{AgentEvent, AgentMessage};
use gglib_core::ports::AgentLoopPort;

use crate::handlers::chat::ChatArgs;

use super::config::build_agent_config;
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
    let config = build_agent_config(args);

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
                // Ctrl+C at the prompt → exit cleanly.
                println!("[exiting]");
                break;
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
        let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);

        let agent = Arc::clone(&agent_loop);
        let msgs = messages.clone();
        let cfg = config.clone();

        let handle: JoinHandle<Option<Vec<AgentMessage>>> = tokio::spawn(async move {
            match agent.run(msgs, cfg, tx).await {
                Ok((_, new_messages)) => Some(new_messages),
                Err(e) => {
                    tracing::debug!("agent loop ended: {e}");
                    None
                }
            }
        });

        // ── 3. Consume event stream; Ctrl+C aborts the agent task ────────────────
        let completed = collect_events(&mut rx, &handle, args.verbose).await;

        // ── 4. Replace conversation history with the loop’s accumulated messages.
        //
        // The loop appends every assistant + tool-result message during the run
        // and includes the final assistant reply, so `new_messages` is the
        // complete context needed for the next turn.
        //
        // On Ctrl+C (`completed = false`) or loop error (handle returns `None`)
        // the history stays unchanged — failed or cancelled turns are not added.
        if completed {
            if let Ok(Some(new_messages)) = handle.await {
                messages = new_messages;
            }
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
/// Returns `true` when the turn completed normally (channel closed or
/// `FinalAnswer` received), `false` when the agent task was aborted by Ctrl+C.
///
/// Callers should only attempt to retrieve accumulated message history from
/// the `handle` when this function returns `true`.
async fn collect_events(
    rx: &mut mpsc::Receiver<AgentEvent>,
    handle: &JoinHandle<Option<Vec<AgentMessage>>>,
    verbose: bool,
) -> bool {
    loop {
        tokio::select! {
            // Prefer processing events over handling Ctrl+C when both are ready.
            biased;

            maybe = rx.recv() => {
                let Some(event) = maybe else { break };

                render_event(&event, verbose);

                if let AgentEvent::FinalAnswer { .. } = event {
                    // NOTE: `FinalAnswer` is always the last event emitted
                    // before the loop drops its `Sender`. Returning here is
                    // safe; any events that arrive after `FinalAnswer` would
                    // indicate a protocol violation and are intentionally
                    // dropped.
                    return true;
                }
            }

            _ = tokio::signal::ctrl_c() => {
                handle.abort();
                // Drain any buffered events without displaying them.
                while rx.try_recv().is_ok() {}
                eprintln!("\n[agent response cancelled — Ctrl+C]");
                return false;
            }
        }
    }
    // Channel closed normally (e.g. loop ended with an error event, no
    // FinalAnswer emitted). History update is still attempted via handle.await.
    true
}
