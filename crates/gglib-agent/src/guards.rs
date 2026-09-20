//! The agent loop's two guards, bundled behind one verdict.
//!
//! [`Guards`] holds a [`StagnationDetector`] and a [`LoopDetector`] and runs
//! both against one iteration's response. It is the only place on this path
//! that sees every guard decision and its outcome, which is why the reporting
//! for #1091 happens here and why this is a file of its own.

use gglib_core::domain::agent::{BatchRecord, LoopDetector, StagnationDetector};
use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::ports::{AgentError, AgentGuardReporter};
use gglib_core::{AgentConfig, AgentEvent, ToolCall};
use tokio::sync::mpsc;

use crate::util::emit_error_event;

/// What the guard decided about one turn.
///
/// Separated from acting on the decision so that every outcome passes one
/// recording point. The alternative — reporting at each of the four exits
/// `check` used to have — is how a counter ends up describing three of them.
enum Decision {
    /// The guard is off for this run: both thresholds are `None`, so no
    /// detector could fire and this turn is not part of any denominator.
    NotScanned,
    /// The guard ran and neither detector fired. Carries the loop detector's
    /// record when it was the one that ran.
    Quiet(Option<BatchRecord>),
    /// A detector fired and the run is over. Carries which one, so the count
    /// does not have to be re-derived from the error.
    Tripped(AgentError, LoopGuardTrip),
}

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
    /// iteration's response, and report the decision.
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
    ///
    /// `reporter` is where the decision is counted (#1091); `None` reports
    /// nowhere and changes nothing else. The recording happens before the
    /// error event is emitted, because emitting awaits a channel the client
    /// may already have closed, and a decision the guard took is a fact
    /// whether or not anyone was still listening for it.
    pub(crate) async fn check(
        &mut self,
        config: &AgentConfig,
        content: &str,
        tool_calls: &[ToolCall],
        tx: &mpsc::Sender<AgentEvent>,
        reporter: Option<&AgentGuardReporter>,
    ) -> Result<Option<BatchRecord>, AgentError> {
        let decision = self.run_detectors(config, content, tool_calls);

        if let Some(reporter) = reporter {
            match &decision {
                Decision::NotScanned => {}
                Decision::Quiet(_) => reporter.sink.record_decision(&reporter.model, None),
                Decision::Tripped(_, which) => {
                    reporter.sink.record_decision(&reporter.model, Some(*which));
                }
            }
        }

        match decision {
            Decision::NotScanned => Ok(None),
            Decision::Quiet(record) => Ok(record),
            Decision::Tripped(e, _) => {
                emit_error_event(tx, &e.to_string()).await;
                Err(e)
            }
        }
    }

    /// Run whichever detectors this run enabled, without acting on the result.
    ///
    /// Synchronous and free of the event channel on purpose: everything that
    /// decides *what happened* is here, and everything that decides *what to
    /// do about it* is in [`Self::check`].
    fn run_detectors(
        &mut self,
        config: &AgentConfig,
        content: &str,
        tool_calls: &[ToolCall],
    ) -> Decision {
        // Whether the guard is on for this *run* — deliberately not whether a
        // detector applies to this particular turn.
        //
        // The proxy records a scan for every request it looks at once the
        // guard is configured, before scanning the history and regardless of
        // what its detectors then do (`loop_guard_step::run`), and records
        // none at all when the guard is off. `agent_guard_scanned` exists to
        // be compared with that number, and two counts are only comparable if
        // they were taken the same way — so the question asked here is the
        // same one: is the guard on?
        if config.max_stagnation_steps.is_none() && config.max_repeated_batch_steps.is_none() {
            return Decision::NotScanned;
        }

        if let Some(max_steps) = config.max_stagnation_steps {
            let did_work = !tool_calls.is_empty();
            if let Err(e) = self.stagnation.record(content, did_work, max_steps) {
                return Decision::Tripped(e, LoopGuardTrip::Stagnation);
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
                    Err(e) => return Decision::Tripped(e, LoopGuardTrip::Loop),
                    Ok(record) => return Decision::Quiet(Some(record)),
                }
            }
        }
        Decision::Quiet(None)
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
