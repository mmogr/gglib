//! The record settings keep of the machine this one paired with.
//!
//! Reading it, writing it, what a write that fails after the code has been
//! spent has to say, and — in [`settle`] — which of those a dial that has
//! come up owes. A sibling rather than more of `connect.rs`, which sits a
//! handful of lines under the file-size budget — and because `mod.rs`'s
//! status surface asks the same question of the same record as `connect`
//! does, and asking it in two places is how the two drift.

use gglib_core::services::AppCore;
use gglib_core::{RemotePairing, SettingsUpdate};
use modelpipe::Ticket;

use crate::error::GuiError;

/// The fingerprint of the machine a stored pairing names, or `None` when
/// this build cannot read the ticket.
///
/// A ticket from a later format is reported as no machine rather than as an
/// error: `remote status` is the command someone runs *because* something is
/// wrong, and it may not be the thing that fails.
pub(super) fn fingerprint(stored: &RemotePairing) -> Option<String> {
    stored
        .ticket
        .parse::<Ticket>()
        .ok()
        .map(|ticket| ticket.fingerprint())
}

/// Whether a stored pairing names the machine `ticket` names.
///
/// By fingerprint rather than by the ticket string: the same machine hands
/// out a different ticket on every `enable` and at every address change, and
/// its endpoint identity is the only part of a ticket that says who will
/// answer. Twelve hex digits is six bytes of an ed25519 public key — enough
/// that grinding a second key to collide with a machine someone has already
/// paired with costs hundreds of core-years, for the privilege of being
/// handed a key the real machine still has to accept.
///
/// A stored ticket this build cannot parse names nobody, which is the safe
/// reading: it costs a re-pair, and never hands a key to a machine that
/// cannot be shown to have issued it.
pub(super) fn names_the_same_machine(stored: &RemotePairing, ticket: &Ticket) -> bool {
    fingerprint(stored).is_some_and(|stored| stored == ticket.fingerprint())
}

/// What a dial that has just come up owes the stored record, and whether it
/// paired.
///
/// The redemption arrives as a closure rather than as a base URL, for the
/// reason the connect watcher takes its statuses as one: `redeem` needs a
/// live tunnel, so a decision reachable only *through* `modelpipe::connect`
/// — an iroh endpoint and a peer that answers — is a comment with an `await`
/// in it rather than something a test can drive. Both arms are decisions
/// worth driving. Which of the two writes takes a redeemed key is the whole
/// reason [`store_redeemed`] is not [`remember`]; and on the codeless arm,
/// that a dial to the machine already recorded writes nothing at all.
///
/// # Errors
///
/// Whatever `redeem` says, and `Internal` when settings cannot be written —
/// in [`store_redeemed`]'s wording on the redeemed arm, because by then the
/// code is gone.
pub(super) async fn settle(
    core: &AppCore,
    ticket: &Ticket,
    held: Option<&RemotePairing>,
    code: Option<String>,
    port: u16,
    redeem: impl AsyncFnOnce(String) -> Result<String, GuiError>,
) -> Result<bool, GuiError> {
    match code {
        Some(code) => {
            let key = redeem(code).await?;
            store_redeemed(core, key, ticket.to_string(), port).await?;
            Ok(true)
        }
        None => {
            // The caller refused a codeless dial with no key for this
            // machine, so `held` is `Some`. Re-storing that key under the
            // ticket just dialled is how the record follows a machine that
            // moved: same identity, new addresses, same key — and under the
            // port just bound, which is how the address a client was
            // configured against stays the address. It is the only write on
            // this arm, so a dial to the machine already recorded, on the
            // port already recorded, touches nothing.
            if let Some(held) =
                held.filter(|held| held.ticket != ticket.to_string() || held.port != Some(port))
            {
                remember(
                    core,
                    RemotePairing {
                        ticket: ticket.to_string(),
                        port: Some(port),
                        ..held.clone()
                    },
                )
                .await?;
            }
            Ok(false)
        }
    }
}

/// Persist what a connection taught us, as one record.
///
/// There is deliberately no way to write half of it. The two were separate
/// `SettingsUpdate` fields, and the codeless arm of `connect` passed `None`
/// for the key — which does not clear the old key, it leaves it exactly
/// where it was, now filed under a ticket for somebody else. The record is
/// taken whole for the same reason: a dial to a machine that moved keeps
/// what this machine remembered about it, and a fresh pairing starts with
/// nothing remembered.
///
/// # Errors
///
/// `Internal` when settings cannot be written.
pub(super) async fn remember(core: &AppCore, pairing: RemotePairing) -> Result<(), GuiError> {
    core.settings()
        .update(SettingsUpdate {
            remote_pairing: Some(Some(pairing)),
            ..SettingsUpdate::default()
        })
        .await
        .map(drop)
        .map_err(|e| GuiError::Internal(format!("could not store the pairing: {e}")))
}

/// Store a pairing whose code has just been redeemed.
///
/// Separate from [`remember`] only in what a failure says, and that is the
/// whole point: by the time this runs the code is gone, so "could not store
/// the pairing" is the one reading a person must not be left with.
///
/// # Errors
///
/// `Internal` when settings cannot be written, in the wording below.
pub(super) async fn store_redeemed(
    core: &AppCore,
    api_key: String,
    ticket: String,
    port: u16,
) -> Result<(), GuiError> {
    remember(
        core,
        RemotePairing {
            ticket,
            api_key,
            default_model: None,
            port: Some(port),
        },
    )
    .await
    .map_err(spent_code)
}

/// A failure to store a pairing whose code has already been spent.
///
/// Worth its own sentence because the ordinary reading of "could not store"
/// is "try again", and trying again cannot work: `redeem` burned the code at
/// both ends of the far machine — the tunnel edge's one-time grant is
/// consumed and the pairing slot cleared — so the next `connect` meets a
/// refusal it reports as an expired or mistyped code. The only way forward
/// is a fresh `enable` there, and saying so is the difference between
/// walking to the other machine once and doing it after an hour of retries.
///
/// It does not get the key back. Nothing at this layer can: the key exists
/// only in the response that was just read, and the store that would have
/// kept it is the thing that failed.
fn spent_code(e: GuiError) -> GuiError {
    GuiError::Internal(format!(
        "{e} — the pairing code was already spent redeeming this key, so `gglib remote connect` \
         cannot be retried with it; run `gglib remote enable --invite` on the other machine for a \
         new one"
    ))
}

#[cfg(test)]
#[path = "stored_pairing_tests.rs"]
mod stored_pairing_tests;

#[cfg(test)]
#[path = "stored_pairing_redial_tests.rs"]
mod stored_pairing_redial_tests;
