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
use gglib_core::domain::agent::{
    AgentConfig, AgentEvent, AgentMessage, MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING,
    MAX_TOOL_TIMEOUT_MS_CEILING, MIN_TOOL_TIMEOUT_MS,
};
use gglib_core::ports::AgentLoopPort;

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
    // Clamp all user-supplied limits to the same [floor, ceiling] ranges as
    // the HTTP handler so the CLI cannot produce an invalid config.
    config.max_iterations = args.max_iterations.clamp(1, MAX_ITERATIONS_CEILING);
    if let Some(ms) = args.tool_timeout_ms {
        config.tool_timeout_ms = ms.clamp(MIN_TOOL_TIMEOUT_MS, MAX_TOOL_TIMEOUT_MS_CEILING);
    }
    if let Some(n) = args.max_parallel {
        config.max_parallel_tools = n.clamp(1, MAX_PARALLEL_TOOLS_CEILING);
    }
    // Defense-in-depth: clamping above guarantees validity, but validate so
    // any future field additions that bypass clamping are caught immediately.
    let config = config
        .validated()
        .map_err(|e| anyhow::anyhow!("invalid agent config: {e}"))?;

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
        messages = run_single_turn(&agent_loop, messages, config.clone(), args.verbose).await;
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
        result = drain_event_stream(&mut rx, verbose) => result,
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
    let mut had_text_delta = false;
    while let Some(event) = rx.recv().await {
        render_event(&event, verbose, had_text_delta);
        if matches!(event, AgentEvent::TextDelta { .. }) {
            had_text_delta = true;
        }

        if let AgentEvent::FinalAnswer { .. } = event {
            // `FinalAnswer` is always the last event emitted before the loop
            // drops its `Sender`.  Any events after this would be a protocol
            // violation and are intentionally dropped.
            debug_assert!(
                rx.try_recv().is_err(),
                "events after FinalAnswer violate agent protocol"
            );
            return true;
        }
    }
    // Channel closed without a FinalAnswer — the loop ended with an error
    // (max iterations, stagnation, etc.).  The caller must not update history.
    false
}
