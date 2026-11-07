//! One turn of a session, through the daemon's door.
//!
//! The daemon runs the only agent loop. A turn is the chat page's: a
//! [`LocalTurn`] put to `PUT /api/runs/{id}?kind=agent`, which the daemon
//! answers on the model, tools and limits the chat's settings save, from the
//! history it keeps, saving the turn as it runs it. Here the run's frames
//! are read back as the [`AgentEvent`]s they were logged from and handed to
//! the session's [`drain`](super::drain), so a reply looks as it always did.
//! Ctrl+C cancels the run on the daemon.

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use gglib_core::domain::agent::AgentEvent;
use gglib_core::domain::hub_chats::LocalTurn;
use gglib_core::domain::runs::{RunInfo, RunStatus, new_run_id};
use tokio::sync::mpsc;

use super::drain::drain_event_stream;
use crate::daemon_client::DaemonHandle;
use crate::daemon_client::runs::RunItem;

/// How a session shows a turn.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shown {
    /// Each iteration's line too.
    pub verbose: bool,
    /// Nothing on stderr: the reply's text alone.
    pub quiet: bool,
}

/// Send `turn` to the daemon and show its reply as it arrives. `true` only
/// when the reply gave its answer.
///
/// # Errors
///
/// The daemon refused the turn before running it (a folder that is not one,
/// a model that cannot read the turn's images, a chat already being
/// answered), in its own words; or it could not be asked.
pub(crate) async fn send(daemon: &DaemonHandle, turn: &LocalTurn, shown: Shown) -> Result<bool> {
    let id = new_run_id();
    let started = daemon.run_turn(&id, turn).await?;
    Ok(follow(daemon, &started.id, shown, tokio::signal::ctrl_c()).await)
}

/// Show run `id`'s reply until it ends, or until `stop` resolves, which
/// cancels the run on the daemon. `true` only when the reply gave its
/// answer.
pub(super) async fn follow(
    daemon: &DaemonHandle,
    id: &str,
    shown: Shown,
    stop: impl Future<Output = std::io::Result<()>>,
) -> bool {
    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
    let failed_aloud = AtomicBool::new(false);
    // Read in this task, beside the drain, not spawned: the channel closes
    // when the run's stream ends, which ends a reply that gave no answer.
    let reading = async {
        let end = read(daemon, id, &tx, &failed_aloud).await;
        drop(tx);
        end
    };
    let shown_reply = async {
        tokio::join!(
            drain_event_stream(&mut rx, shown.verbose, shown.quiet),
            reading
        )
    };
    let (completed, end) = tokio::select! {
        biased;
        both = shown_reply => both,
        _ = stop => {
            if let Err(e) = daemon.run_cancel(id).await {
                tracing::warn!("the run could not be cancelled: {e}");
            }
            eprintln!("\n[agent response cancelled — Ctrl+C]");
            return false;
        }
    };
    if let Ok(Some(end)) = end
        && !failed_aloud.load(Ordering::SeqCst)
        && let Some(line) = failure(&end)
    {
        eprintln!("\n  ✗  {line}");
    }
    completed
}

/// Read run `id`'s frames into `tx` as events until the run ends, and
/// return its final state. A frame that is not an event this build knows
/// is skipped. `said` is set when an error event is among them, which then
/// says why the run failed.
async fn read(
    daemon: &DaemonHandle,
    id: &str,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    said: &AtomicBool,
) -> Result<Option<RunInfo>> {
    daemon
        .run_events(id, 0, |item| {
            let RunItem::Frame { data, .. } = item else {
                return ControlFlow::Continue(());
            };
            match serde_json::from_str::<AgentEvent>(data) {
                Ok(event) => {
                    if matches!(event, AgentEvent::Error { .. }) {
                        said.store(true, Ordering::SeqCst);
                    }
                    if tx.send(event).is_err() {
                        return ControlFlow::Break(());
                    }
                }
                Err(e) => tracing::debug!("skipping a frame this build cannot read: {e}"),
            }
            ControlFlow::Continue(())
        })
        .await
}

/// Why a run that ended failed, in the daemon's words; `None` for one that
/// did not fail.
fn failure(end: &RunInfo) -> Option<String> {
    if end.status != RunStatus::Failed {
        return None;
    }
    Some(end.error.as_ref().map_or_else(
        || "the reply failed, and the daemon gave no reason".to_owned(),
        |e| e.message.clone(),
    ))
}

#[cfg(test)]
#[path = "turn_stand_in.rs"]
pub(crate) mod turn_stand_in;
#[cfg(test)]
#[path = "turn_tests.rs"]
mod turn_tests;
