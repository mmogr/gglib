//! A reply the upstream cut off at its token limit.
//!
//! llama-server stops a reply at the request's `max_tokens`, or where the
//! context runs out, and says so with `finish_reason: "length"`. A reply cut
//! off while it wrote a tool call, or before it wrote any text, has nothing
//! the loop can use: the call never closed, and thinking alone is no answer.
//! The loop ends the run on it, with an `error` event and
//! [`AgentError::TurnTooLong`], rather than run that call or send any of it
//! back to the model. A reply cut off in its text is taken as if it had
//! stopped (its text is the answer, unless the path demanded a call), and
//! the run says it was cut off with one `system_warning`
//! ([`is_cut_off_warning`] tells it from the others). The words after a
//! Draw's picture are the exception: they are always answered for
//! ([`IfWordless::Answer`]).

use gglib_core::AgentEvent;
use gglib_core::ports::AgentError;
use tokio::sync::mpsc;
use tracing::warn;

use crate::stream_collector::CollectedResponse;
use crate::util::emit_error_event;

/// The `finish_reason` of a turn cut off at its token limit.
pub(crate) const CUT_OFF: &str = "length";

/// How every cut-off warning's message starts, the mark
/// [`is_cut_off_warning`] reads.
const CUT_OFF_WARNING: &str = "The answer was cut off at its token limit";

/// What a reply's path does with one cut off while it wrote a call, or
/// before it wrote any text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IfWordless {
    /// End the run: the reply has nothing to give.
    EndTheRun,
    /// Keep it: the run's demanded call has run, its calls are never run
    /// again, and the path answers a wordless reply with words of its own
    /// (`FirstCall::if_wordless`), so even a reply cut off while it wrote a
    /// call (its text dropped) does not fail a run whose work is done.
    Answer,
}

/// `reply`, unless it was cut off at its token limit and cannot be used.
///
/// A reply that stopped is handed back untouched. When `if_wordless` says
/// [`IfWordless::EndTheRun`], one cut off while it wrote a call, or with no
/// text, ends the run ([`end_on_cut_off`]). When it says
/// [`IfWordless::Answer`], one cut off while it wrote a call is handed back
/// with no text, for the path to answer for. Any other cut-off reply is
/// handed back as if it had stopped. Either is handed back after its
/// [`cut_off_warning`] is sent.
pub(crate) async fn usable(
    mut reply: CollectedResponse,
    if_wordless: IfWordless,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<CollectedResponse, AgentError> {
    if reply.finish_reason.as_deref() != Some(CUT_OFF) {
        return Ok(reply);
    }
    if if_wordless == IfWordless::EndTheRun && (reply.call_cut || reply.content.trim().is_empty()) {
        return end_on_cut_off(tx, reply.completion_tokens).await;
    }
    if if_wordless == IfWordless::Answer && reply.call_cut {
        // The text before a runaway call led into that call; it is no answer.
        reply.content.clear();
    }
    let wordless = reply.content.trim().is_empty();
    warn!(
        completion_tokens = ?reply.completion_tokens,
        wordless,
        "the model's answer was cut off at its token limit"
    );
    let _ = tx.send(cut_off_warning(reply.completion_tokens)).await;
    Ok(reply)
}

/// End the run on a turn cut off after `completion_tokens`, when the upstream
/// reported them: emit its `error` event, then answer with
/// [`AgentError::TurnTooLong`].
pub(crate) async fn end_on_cut_off<T>(
    tx: &mpsc::Sender<AgentEvent>,
    completion_tokens: Option<u32>,
) -> Result<T, AgentError> {
    warn!(
        ?completion_tokens,
        "the model's turn was cut off at its token limit"
    );
    let after = completion_tokens.map_or_else(String::new, |n| format!(" after {n} tokens"));
    let message = format!(
        "The model's turn ran too long: it was cut off at its token limit{after}, \
         before it finished, so the run was stopped."
    );
    emit_error_event(tx, &message).await;
    Err(AgentError::TurnTooLong { completion_tokens })
}

/// The `system_warning` a run sends for an answer cut off after
/// `completion_tokens`, when the upstream reported them.
#[must_use]
pub fn cut_off_warning(completion_tokens: Option<u32>) -> AgentEvent {
    let after = completion_tokens.map_or_else(String::new, |n| format!(" after {n} tokens"));
    AgentEvent::SystemWarning {
        message: format!("{CUT_OFF_WARNING}{after}, before it finished."),
        suggested_action: Some("Raise max_tokens to give it more room.".to_owned()),
    }
}

/// Whether `event` is the warning for an answer cut off at its token limit
/// ([`cut_off_warning`]), which a benchmark does not count as a runaway.
#[must_use]
pub fn is_cut_off_warning(event: &AgentEvent) -> bool {
    matches!(event, AgentEvent::SystemWarning { message, .. } if message.starts_with(CUT_OFF_WARNING))
}
