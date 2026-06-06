//! Background stdin routing task for the `council` command family.
//!
//! A single background tokio task owns `tokio::io::stdin()` and routes each
//! typed line to one of two destinations:
//!
//! - Lines starting with `/note ` have the prefix stripped and the body
//!   pushed into the shared [`NoteQueue`].  The executor drains this queue at
//!   each wave boundary to produce a steering [`GraphDiff`] via the steering
//!   LLM call.
//! - All other lines are forwarded over an [`mpsc::UnboundedSender`] so that
//!   the approval handler in [`crate::handlers::council::approve`] can read
//!   them as user responses.
//!
//! Only one input router should be running per `council run` / `resume` /
//! `rewind` invocation.  Spawn it once before the event loop; the returned
//! receiver is passed down through `render_event` into `prompt_and_resolve`.

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use gglib_agent::council::NoteQueue;

/// Spawn the background stdin router and return the approval-input receiver
/// alongside a [`tokio::task::JoinHandle`] for the router task.
///
/// Lines starting with `/note ` are stripped and pushed to `note_queue`.
/// All other lines are sent to the returned [`mpsc::UnboundedReceiver`].
///
/// The task runs until stdin is closed (EOF) **or until the handle is
/// aborted**.  Callers must call `handle.abort()` when the council run
/// finishes so that the task does not keep the tokio runtime alive
/// indefinitely while blocking on a TTY stdin.
pub(crate) fn spawn_input_router(
    note_queue: NoteQueue,
) -> (mpsc::UnboundedReceiver<String>, tokio::task::JoinHandle<()>) {
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let handle = tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut lines = BufReader::new(stdin).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(rest) = line.strip_prefix("/note ") {
                let note = rest.trim().to_owned();
                if !note.is_empty() {
                    note_queue.lock().await.push(note);
                }
            } else if tx.send(line).is_err() {
                // Receiver dropped — stop routing non-note input.
                break;
            }
        }
    });
    (rx, handle)
}
