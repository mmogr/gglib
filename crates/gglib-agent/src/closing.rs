//! The end of a run that has used all its steps.
//!
//! A run that reaches `max_iterations` without a final answer makes one more
//! request, with no tools offered, so the model can give its answer from what
//! it has found. Before it, the run sends a `system_warning` that its steps
//! are used up. The request ends with a `user` note that says so, in a copy
//! of the messages, so the note is in no event and not in the history the run
//! returns. The reply ends the run as its final answer. The loop and
//! stagnation guards do not judge it, as they do not judge the words after a
//! Draw's picture.
//!
//! The run ends as it did before this request, with an `error` event and
//! [`AgentError::MaxIterationsReached`], when the reply is no use: the
//! request fails, its text is empty, or its text starts with a known
//! tool-call marker ([`KNOWN_RESIDUE_MARKERS`]), which is what a model that
//! tries a call with no tools offered writes. A run with a demanded first
//! call (`AgentConfig::first_call`) makes no closing request.

use gglib_core::normalize::residue::KNOWN_RESIDUE_MARKERS;
use gglib_core::ports::{AgentError, AgentRunOutput, LlmCompletionPort};
use gglib_core::{AgentConfig, AgentEvent, AgentMessage};
use tokio::sync::mpsc;
use tracing::warn;

use crate::agent_loop::AgentLoop;
use crate::context_pruning::Pruned;
use crate::cut_off::{IfWordless, usable};
use crate::stream_collector::{CollectedResponse, collect_stream};
use crate::turn_usage::measure_turn;
use crate::util::emit_error_event;

/// The note that ends the closing request of a run of `n` steps.
fn note(n: usize) -> String {
    format!(
        "You have used all {n} of this run's steps and can make no more tool calls. Give your \
         answer now from what you have found. If you cannot answer yet, say what you have \
         established and what is left to do, and ask for more steps."
    )
}

/// How every step-limit warning's message starts, before its step count.
const STEP_LIMIT_WARNING: &str = "This run used all ";

/// How every step-limit warning's message ends, after its step count.
const STEP_LIMIT_WARNING_END: &str =
    " of its steps. It was asked to answer without further tool calls.";

/// The `system_warning` a run of `n` steps sends before its closing request.
#[must_use]
pub fn step_limit_warning(n: usize) -> AgentEvent {
    AgentEvent::SystemWarning {
        message: format!("{STEP_LIMIT_WARNING}{n}{STEP_LIMIT_WARNING_END}"),
        suggested_action: None,
    }
}

/// Whether `event` is the warning a run sends when it has used all its
/// steps ([`step_limit_warning`]), which a benchmark does not count as a
/// runaway.
#[must_use]
pub fn is_step_limit_warning(event: &AgentEvent) -> bool {
    matches!(event, AgentEvent::SystemWarning { message, .. }
        if message.starts_with(STEP_LIMIT_WARNING) && message.ends_with(STEP_LIMIT_WARNING_END))
}

/// End a run that has used all `config.max_iterations` of its steps.
///
/// Sends the warning, makes the closing request, and ends the run with its
/// reply as the final answer, its completion tokens added to
/// `total_completion_tokens`. The run reports `max_iterations` iterations:
/// the closing request is not a step. Ends the run with an `error` event and
/// [`AgentError::MaxIterationsReached`] when the reply is no use, and at
/// once, with no request, in a run with a demanded first call.
pub(crate) async fn end_at_limit(
    llm: &dyn LlmCompletionPort,
    messages: &mut Pruned,
    config: &AgentConfig,
    total_completion_tokens: Option<u64>,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<AgentRunOutput, AgentError> {
    let n = config.max_iterations;
    warn!(max = n, "agent loop hit max iterations");
    if config.first_call.is_none()
        && let Some(reply) = closing_reply(llm, messages, n, tx).await
    {
        let total = reply
            .completion_tokens
            .map_or(total_completion_tokens, |ct| {
                Some(total_completion_tokens.unwrap_or(0) + u64::from(ct))
            });
        return AgentLoop::finalize_answer(messages, reply.content, n - 1, total, tx).await;
    }
    let error = AgentError::MaxIterationsReached(n);
    emit_error_event(tx, &error.to_string()).await;
    Err(error)
}

/// Send the warning, ask the model for its answer with no tools offered, and
/// hand back the reply when it is of use.
///
/// A failure of the request or of its stream is logged and answered with
/// `None`, with no `error` event of its own: the run's `error` event is the
/// limit's. (The collector sends one of its own for a tool call it cannot
/// read, such as one whose arguments are not JSON or that has no id or
/// name, as it does on every path.) A reply cut off at its token limit is
/// kept as [`IfWordless::Answer`] keeps it, with the cut-off warning. A tool
/// call in the reply all the same is not run, and is logged.
async fn closing_reply(
    llm: &dyn LlmCompletionPort,
    messages: &Pruned,
    n: usize,
    tx: &mpsc::Sender<AgentEvent>,
) -> Option<CollectedResponse> {
    let _ = tx.send(step_limit_warning(n)).await;
    let mut asked = messages.to_vec();
    asked.push(AgentMessage::user(note(n)));
    let stream = match llm.chat_stream(&asked, &[]).await {
        Ok(stream) => stream,
        Err(e) => {
            warn!(error = %format!("{e:#}"), "the closing request at the step limit failed");
            return None;
        }
    };
    let reply = match collect_stream(measure_turn(stream, tx.clone(), messages.dropped()), tx).await
    {
        Ok(reply) => reply,
        Err(e) => {
            warn!(error = %format!("{e:#}"), "the closing reply at the step limit failed");
            return None;
        }
    };
    let reply = usable(reply, IfWordless::Answer, tx).await.ok()?;
    if !reply.tool_calls.is_empty() {
        warn!(
            ignored = reply.tool_calls.len(),
            "a reply that was offered no tools called some; none is run"
        );
    }
    let text = reply.content.trim();
    if text.is_empty() {
        warn!("the closing reply at the step limit had no text");
        return None;
    }
    if let Some(marker) = KNOWN_RESIDUE_MARKERS.iter().find(|m| text.starts_with(**m)) {
        warn!(
            marker,
            "the closing reply at the step limit was a tool call in its text"
        );
        return None;
    }
    Some(reply)
}
