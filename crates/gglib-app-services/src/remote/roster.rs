//! The roster's persistence, kept off the request path.
//!
//! `RemoteGatewayPort` is synchronous by contract — "every implementation is
//! a lock and a counter, and a port that forces an `await` on the request
//! path for that would be paying for nothing" — and the gateway holds
//! neither `AppCore` nor the tunnel handle. But a device's label and when it
//! was last seen do have to reach settings eventually.
//!
//! So they do not travel by return value. The gateway records a [`Note`] and
//! drops it down an unbounded channel, whose `send` is synchronous and does
//! not block; `roster_sync` owns the other end, runs beside `rotation_poll`
//! for the life of a session, and does the writing. What is lost if the
//! daemon dies between the two is a label and a timestamp — both advisory.
//! The key and the token were durable before the code was ever offered.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gglib_core::services::AppCore;
use gglib_core::{Device, SettingsUpdate};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::warn;

use super::gateway::RemoteGateway;
use crate::error::GuiError;

/// Something the roster should be told, once somebody can write it.
///
/// The port these arrive through is synchronous by contract — it is a lock
/// and a counter on the request path — so nothing here may block or await.
/// `roster_sync` owns the other end and does the settings write.
#[derive(Debug, Clone)]
pub(crate) enum Note {
    /// A device redeemed an invite and said what it is called.
    Joined {
        /// The name the edge holds its key under.
        device: String,
        /// What it calls itself, when it said.
        label: Option<String>,
    },
    /// A request arrived bearing a device's token.
    Seen {
        /// The name the edge admitted it under.
        device: String,
        /// Unix milliseconds.
        at_ms: i64,
    },
}

/// Open the channel, hand the gateway its end, and spawn the writer.
///
/// One call from `arm`, so the two ends cannot be wired up apart.
pub(super) fn start(gateway: &Arc<RemoteGateway>, core: Arc<AppCore>, lock: Arc<Mutex<()>>) {
    let (notes, inbox) = tokio::sync::mpsc::unbounded_channel();
    gateway.take_notes(notes);
    tokio::spawn(roster_sync(core, lock, inbox));
}

/// Persist roster notes for the life of a session.
///
/// Spawned by [`start`] beside `rotation_poll`, and ended by the teardown —
/// `reset_session_if` drops the gateway's sender, which closes the channel.
/// That is deliberately the *only* way out: a cancellation token would cut
/// the queue off mid-flush, and the notes still in it are the ones the drain
/// just produced. What ends this task is therefore also what has already
/// stopped anything adding to it.
///
/// Two rules it enforces on the way through, both of which exist because the
/// notes come from the request path and a local process can forge the marker
/// headers:
///
/// - **An id the roster does not hold is dropped**, in [`apply`]. A forged
///   `X-Modelpipe-Device` cannot invent a device or move one this machine
///   never issued a key to.
/// - **`last_seen` is debounced.** A device under load would otherwise be one
///   settings write per request, and the value is advisory to the minute.
async fn roster_sync(core: Arc<AppCore>, lock: Arc<Mutex<()>>, mut notes: UnboundedReceiver<Note>) {
    let mut last_written: HashMap<String, i64> = HashMap::new();
    while let Some(note) = notes.recv().await {
        match note {
            Note::Joined { device, label } => {
                if let Err(e) = apply(&core, &lock, &device, |d| d.label.clone_from(&label)).await {
                    warn!(device = %device, "could not record what a device calls itself: {e}");
                }
            }
            Note::Seen { device, at_ms } => {
                // Advisory to the minute; a busy device is not worth a
                // settings write per request.
                let recent = last_written
                    .get(&device)
                    .is_some_and(|written| at_ms - written < DEBOUNCE_MS);
                if recent {
                    continue;
                }
                match apply(&core, &lock, &device, |d| d.last_seen = Some(at_ms)).await {
                    Ok(true) => {
                        last_written.insert(device, at_ms);
                    }
                    Ok(false) => {}
                    Err(e) => warn!(device = %device, "could not record a device as seen: {e}"),
                }
            }
        }
    }
}

/// How long between `last_seen` writes for one device.
const DEBOUNCE_MS: i64 = 60_000;

/// Change one roster row, if the roster holds it. `false` means it did not —
/// which is the forged-marker case, and is not an error.
async fn apply(
    core: &AppCore,
    lock: &Mutex<()>,
    device: &str,
    change: impl FnOnce(&mut Device),
) -> Result<bool, GuiError> {
    let _guard = lock.lock().await;
    let mut roster = read_roster(core).await?;
    let Some(row) = roster.iter_mut().find(|d| d.id == device) else {
        return Ok(false);
    };
    change(row);
    write_roster(core, roster).await?;
    Ok(true)
}

/// The roster as settings holds it. Absent reads as empty: a machine that has
/// never invited anything and one whose roster was emptied are the same
/// machine, and neither admits a device.
///
/// # Errors
///
/// `Internal` when settings cannot be read.
pub(super) async fn read_roster(core: &AppCore) -> Result<Vec<Device>, GuiError> {
    let settings = core
        .settings()
        .get()
        .await
        .map_err(|e| GuiError::Internal(format!("could not read the device roster: {e}")))?;
    Ok(settings.remote_devices.unwrap_or_default())
}

/// Replace the roster whole.
///
/// `remote_devices` merges by replacement, not by row, so every caller reads
/// the whole list, changes it and writes it back — which is why they all do
/// so under `RemoteOps::roster`.
///
/// # Errors
///
/// `Internal` when settings cannot be written.
pub(super) async fn write_roster(core: &AppCore, roster: Vec<Device>) -> Result<(), GuiError> {
    core.settings()
        .update(SettingsUpdate {
            remote_devices: Some(Some(roster)),
            ..SettingsUpdate::default()
        })
        .await
        .map_err(|e| GuiError::Internal(format!("could not write the device roster: {e}")))?;
    Ok(())
}

/// Unix milliseconds, or 0 for a clock before the epoch — which is a wrong
/// timestamp on a row a person reads, not a reason to refuse an invite.
pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}
