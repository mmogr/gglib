//! The agent loop's two guards, bundled behind one verdict.
//!
//! [`Guards`] holds a [`StagnationDetector`] and a [`LoopDetector`] and runs
//! both against one iteration's response. It is the only place on this path
//! that sees every guard decision and its outcome, which is why it lives in a
//! file of its own rather than beside the state machine that calls it.
//!
//! Moved here from `agent_loop`, unchanged.

use gglib_core::domain::agent::{BatchRecord, LoopDetector, StagnationDetector};
use gglib_core::ports::AgentError;
use gglib_core::{AgentConfig, AgentEvent, ToolCall};
use tokio::sync::mpsc;

use crate::util::emit_error_event;

/// Bundles the stagnation and loop-detection detectors so they can be passed
/// as a single unit rather than two independent `&mut` parameters.
///
/// Guards whose corresponding `Option` field in [`AgentConfig`] is `None` are
/// skipped entirely — `None` disables the guard (e.g. in tests that reuse a
/// fixed LLM response or deliberately repeat the same tool call batch).
#[derive(Default)]
pub(crate) struct Guards {
    stagnation: StagnationDetector,
    loop_detector: LoopDetector,
}

impl Guards {
    /// Check both stagnation and loop-detection guards against the current
    /// iteration's response.
    ///
    /// Stagnation is checked only on iterations that made **no** tool calls —
    /// a turn that called a tool is doing work, and the loop detector judges it.
    ///
    /// Loop detection is only checked when tool calls are present, since an
    /// empty batch would produce a degenerate signature — and, now that the
    /// detector counts back-to-back repeats, skipping is also what stops a
    /// text-only iteration from breaking a run. See `loop_detection`.
    ///
    /// On failure, emits an [`AgentEvent::Error`] on `tx` before returning so
    /// SSE consumers always see the failure reason before the stream closes.
    ///
    /// Returns the [`BatchRecord`] for the batch the loop detector counted, so
    /// the caller can hand its answers back once they exist. `None` when there
    /// was no batch to count — an empty `tool_calls`, or loop detection
    /// disabled — which [`Self::record_results`] accepts and ignores, so the
    /// caller does not branch on it.
    pub(crate) async fn check(
        &mut self,
        config: &AgentConfig,
        content: &str,
        tool_calls: &[ToolCall],
        tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<Option<BatchRecord>, AgentError> {
        if let Some(max_steps) = config.max_stagnation_steps {
            let did_work = !tool_calls.is_empty();
            if let Err(e) = self.stagnation.record(content, did_work, max_steps) {
                emit_error_event(tx, &e.to_string()).await;
                return Err(e);
            }
        }
        if !tool_calls.is_empty() {
            if let Some(max_steps) = config.max_repeated_batch_steps {
                match self.loop_detector.check(
                    tool_calls,
                    max_steps,
                    &config.observation_tools,
                    config.max_observation_steps,
                ) {
                    Err(e) => {
                        emit_error_event(tx, &e.to_string()).await;
                        return Err(e);
                    }
                    Ok(record) => return Ok(Some(record)),
                }
            }
        }
        Ok(None)
    }

    /// Hand the answers back to the loop detector.
    ///
    /// Separate from [`Self::check`] because on this path the batch has not run
    /// at check time, so its answers do not exist yet. The proxy's history scan
    /// calls both together; this one cannot.
    ///
    /// The caller must reach this with no `?` between the execution and here:
    /// the answers recorded belong to the batch that *just ran*, and the
    /// comparison at the next `check` is against those. Recording a different
    /// batch's would invert the measurement silently — which is what
    /// [`BatchRecord`] exists to make impossible, and why this takes one
    /// rather than a bare signature.
    pub(crate) fn record_results(&mut self, record: Option<BatchRecord>, answers: Option<u64>) {
        if let Some(record) = record {
            self.loop_detector.record_results(record, answers);
        }
    }
}
