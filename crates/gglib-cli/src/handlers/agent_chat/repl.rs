//! Async REPL loop for `gglib chat`.
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
//! A Ctrl+C **during an agent response** is handled in [`super::turn`]: the
//! daemon's run is cancelled, and the REPL returns to the prompt.
//!
//! A Ctrl+C **at the readline prompt** is signalled by rustyline as
//! [`ReadlineError::Interrupted`]; the REPL prints a hint and continues to
//! the next prompt instead of exiting (keeping behaviour consistent with the
//! help text — `/quit` or Ctrl+D are the intended exit paths).

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

use gglib_core::domain::Thinking;
use gglib_core::domain::attachment::AttachmentId;
use gglib_core::domain::hub_chats::LocalTurn;

use crate::daemon_client::DaemonHandle;

use super::branches;
use super::draw::DrawSwitch;
use super::images::TurnImages;
use super::persistence::Conversation;
use super::repl_line::{self, Line};
use super::turn::{self, Shown};

// =============================================================================
// Help text
// =============================================================================

const REPL_HELP: &str = "\
  /help     print this message
  /image <path>  attach a PNG or JPEG to your next message
  /retry    answer the last question again; a saved reply is kept, on its own branch
  /edit <text>   ask the last question again as <text>; a reply to it is kept, on its own branch
  /branch   copy the chat into a new branch and go on there
  /branches list the other branches along the chat
  /draw     let the model draw a picture for your next message
  /draw <model>  the same, with that image model (a name or id) from now on
  /draw default  the same, with the daemon's default image model again
  /draw list     list the daemon's image models
  /quit     exit the session
  /exit     exit the session
  Ctrl+C    cancel the current agent response (return to prompt)
  Ctrl+D    exit the session (EOF)";

// =============================================================================
// Public entry point
// =============================================================================

/// A session at the prompt: the daemon its turns go to, the conversation
/// they are added to, and what each turn carries besides what is typed.
pub(crate) struct Session<'a> {
    pub daemon: DaemonHandle,
    pub conversation: Conversation<'a>,
    /// The images attached to the next message.
    pub images: TurnImages<'a>,
    /// The Draw switch (`/draw`).
    pub draw: DrawSwitch,
    /// The folder each turn says (`files`); `None` says none.
    pub folder: Option<String>,
    /// The Thinking choice this command line named, said on each turn.
    pub thinking: Option<Thinking>,
    pub shown: Shown,
}

impl Session<'_> {
    /// The turn that adds `content` and `images` to the conversation, or,
    /// with `answer_saved`, answers the question it already ends in.
    fn turn(&self, content: String, images: Vec<AttachmentId>, answer_saved: bool) -> LocalTurn {
        let (draw, image_model) = self.draw.turn();
        LocalTurn {
            conversation_id: self.conversation.id,
            content,
            images,
            thinking: self.thinking,
            answer_saved,
            draw,
            image_model,
            folder: self.folder.clone(),
        }
    }

    /// Send `turn`, and show its reply. Draw is for one message, so whatever
    /// the turn did, finished, failed, refused or cancelled, the message
    /// after it is offered no image tool. `true` when the reply gave its
    /// answer; a refusal is said on stderr.
    pub(crate) async fn send(&self, turn: &LocalTurn) -> bool {
        let sent = turn::send(&self.daemon, turn, self.shown).await;
        self.draw.sent();
        sent.unwrap_or_else(|refused| {
            eprintln!("  ✗  {refused}");
            false
        })
    }
}

/// Run the interactive agent REPL until the user quits or reaches EOF.
///
/// Each message typed is a turn on the session's conversation, which the
/// daemon answers from the history it keeps. Also called by `gglib q` to go
/// on from a question as a chat.
pub(crate) async fn run_repl(mut session: Session<'_>) -> Result<()> {
    // Wrap the editor in Arc<Mutex> so it can be moved into spawn_blocking
    // on each turn while retaining readline history across turns.
    let editor: Arc<Mutex<DefaultEditor>> =
        Arc::new(Mutex::new(DefaultEditor::new().map_err(|e| {
            anyhow::anyhow!("failed to initialise readline editor: {e}")
        })?));

    println!("Agentic chat ready. Type /help for help, /quit to exit.");
    let id = session.conversation.id;
    println!(
        "{}Session #{id} — resume later with: gglib chat --continue {id}{}",
        crate::presentation::style::DIM,
        crate::presentation::style::RESET,
    );

    // ── REPL outer loop ──────────────────────────────────────────────────────
    loop {
        // ── 1. Read user input (blocking → spawn_blocking) ───────────────────
        let ed = Arc::clone(&editor);
        let line = tokio::task::spawn_blocking(move || {
            ed.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
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

        let turn = match repl_line::read(input.trim(), &mut session.images).await {
            Line::Empty => continue,
            Line::Quit => break,
            Line::Help => {
                println!("{REPL_HELP}");
                continue;
            }
            Line::Image(reply) => {
                eprintln!("{reply}");
                continue;
            }
            Line::Branches => {
                branches::list(&session.conversation).await;
                continue;
            }
            Line::Change(ask) => {
                // A change that leaves a question to answer is answered by
                // a turn that adds no message.
                if !branches::go(&mut session.conversation, ask).await {
                    continue;
                }
                session.turn(String::new(), Vec::new(), true)
            }
            Line::Draw(what) => {
                eprintln!("{}", session.draw.say(what.as_deref()).await);
                continue;
            }
            Line::Send { content, images } => session.turn(content, images, false),
        };
        session.send(&turn).await;
    }

    let id = session.conversation.id;
    println!(
        "{}Session #{id} saved. Resume with: gglib chat --continue {id}{}",
        crate::presentation::style::DIM,
        crate::presentation::style::RESET,
    );

    Ok(())
}

#[cfg(test)]
#[path = "repl_turn_tests.rs"]
mod repl_turn_tests;
