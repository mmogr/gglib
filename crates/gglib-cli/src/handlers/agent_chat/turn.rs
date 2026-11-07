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
use std::time::Duration;

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
    let followed = follow(daemon, &started.id, shown, tokio::signal::ctrl_c()).await;
    if let Some(line) = &followed.line {
        eprintln!("\n{line}");
    }
    Ok(followed.answered)
}

/// How long a cancelled run is waited for to end, so the next turn on its
/// chat is not refused as one still being answered.
const CANCEL_WAIT: Duration = Duration::from_secs(30);

/// How a reply that was followed ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Followed {
    /// Whether it gave its answer.
    pub answered: bool,
    /// What to say of how it ended, on stderr, when there is anything.
    pub line: Option<String>,
}

/// Show run `id`'s reply until it ends, or until `stop` resolves, which
/// cancels the run on the daemon and waits for it to end there.
pub(super) async fn follow(
    daemon: &DaemonHandle,
    id: &str,
    shown: Shown,
    stop: impl Future<Output = std::io::Result<()>>,
) -> Followed {
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
    let (answered, end) = tokio::select! {
        biased;
        both = shown_reply => both,
        _ = stop => {
            if let Err(e) = daemon.run_cancel(id).await {
                tracing::warn!("the run could not be cancelled: {e}");
            }
            // Its reply is saved when it ends, and a turn on the chat before
            // then would be refused.
            let ended = daemon.run_events(id, 0, |_| ControlFlow::Continue(()));
            if tokio::time::timeout(CANCEL_WAIT, ended).await.is_err() {
                tracing::warn!("the cancelled run {id} did not end within {CANCEL_WAIT:?}");
            }
            let line = Some("[agent response cancelled — Ctrl+C]".to_owned());
            return Followed { answered: false, line };
        }
    };
    let said = failed_aloud.load(Ordering::SeqCst);
    let line = (!answered).then(|| ending(id, &end, said)).flatten();
    Followed { answered, line }
}

/// What to say of a reply that gave no answer, by how its stream ended:
/// a run that failed, unless an error event said why; a stream that ended,
/// or could not be read, before the run did, with how to read the rest.
fn ending(id: &str, end: &Result<Option<RunInfo>>, failed_aloud: bool) -> Option<String> {
    let rest = format!("`gglib run show {id} --follow` reads the rest");
    match end {
        Ok(Some(_)) if failed_aloud => None,
        Ok(Some(info)) => failure(info).map(|why| format!("  ✗  {why}")),
        Ok(None) => Some(format!(
            "  ✗  the daemon's stream ended before the reply did; {rest}"
        )),
        Err(e) => Some(format!(
            "  ✗  the reply could not be read from the daemon ({e:#}); {rest}"
        )),
    }
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
