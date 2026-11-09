//! Bridges the completion adapter's retry activity onto an agent run's events.
//!
//! The adapter backs off several layers below this handler and has no idea a
//! user is waiting. Without a bridge, a contended model looks identical to a
//! hung one: no tokens, no explanation, for as long as the retry budget lasts.
//!
//! [`AgentEvent::SystemWarning`] is the right carrier — it is documented as
//! non-fatal and does not terminate the loop, so the notice arrives mid-stream
//! and generation continues afterwards.

use std::time::Duration;

use tokio::sync::mpsc;

use gglib_core::domain::agent::AgentEvent;
use gglib_core::ports::RetryObserver;

/// Turns retry callbacks into `SystemWarning` frames on the agent's channel.
pub(super) struct RetryNotice {
    events: mpsc::Sender<AgentEvent>,
}

impl RetryNotice {
    /// Bridge onto `events`, the same channel the agent loop emits through.
    pub(super) fn new(events: mpsc::Sender<AgentEvent>) -> Self {
        Self { events }
    }

    /// Queue a notice, dropping it if the consumer has fallen behind.
    ///
    /// `try_send` rather than an await: [`RetryObserver`] is called from inside
    /// the request path, so blocking here would delay the very retry being
    /// announced. A dropped notice costs the user a status line; a stalled
    /// retry costs them the request.
    fn emit(&self, message: String) {
        let event = AgentEvent::SystemWarning {
            message,
            suggested_action: None,
        };
        if let Err(e) = self.events.try_send(event) {
            tracing::debug!(error = %e, "agent: dropped a retry notice; consumer is behind");
        }
    }
}

impl RetryObserver for RetryNotice {
    fn on_retry(&self, attempt: u32, delay: Duration, reason: &str) {
        // The full upstream reason is already on the tracing span from the
        // adapter. What reaches the UI stays short enough to read at a glance.
        tracing::debug!(attempt, reason, "agent: surfacing retry notice");
        self.emit(format!(
            "Model unavailable — retrying in {:.1}s (attempt {attempt})",
            delay.as_secs_f64()
        ));
    }

    fn on_exhausted(&self, attempts: u32, elapsed: Duration, reason: &str) {
        // A terminal error follows this immediately. The notice still earns its
        // place: it says how long was spent trying, which the error alone does
        // not make obvious to someone who has been watching a blank panel.
        tracing::debug!(attempts, reason, "agent: surfacing retry exhaustion");
        self.emit(format!(
            "Model still unavailable after {attempts} attempts over {:.0}s — giving up",
            elapsed.as_secs_f64()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words a run's notice says, in order, from the events it is sent.
    fn said(rx: &mut mpsc::Receiver<AgentEvent>) -> Vec<String> {
        let mut said = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                AgentEvent::SystemWarning { message, .. } => said.push(message),
                other => panic!("not a notice: {other:?}"),
            }
        }
        said
    }

    /// A backoff and the give-up each reach the events the loop sends, the
    /// same channel the run logs, as a `system_warning` in these words.
    #[test]
    fn a_retry_and_its_give_up_reach_the_runs_events() {
        let (tx, mut rx) = mpsc::channel(4);
        let notice = RetryNotice::new(tx);

        notice.on_retry(2, Duration::from_millis(1500), "503");
        notice.on_exhausted(4, Duration::from_secs(31), "deadline");

        assert_eq!(
            said(&mut rx),
            [
                "Model unavailable — retrying in 1.5s (attempt 2)",
                "Model still unavailable after 4 attempts over 31s — giving up",
            ]
        );
    }
}
