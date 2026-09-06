//! The record settings keep of the machine this one paired with.
//!
//! Reading it, writing it, and what a write that fails after the code has
//! been spent has to say. A sibling rather than more of `connect.rs`, which
//! sits a handful of lines under the file-size budget — and because
//! `mod.rs`'s status surface asks the same question of the same record as
//! `connect` does, and asking it in two places is how the two drift.

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

/// Persist what a connection taught us, as one record.
///
/// There is deliberately no way to write half of it. The two were separate
/// `SettingsUpdate` fields, and the codeless arm of `connect` passed `None`
/// for the key — which does not clear the old key, it leaves it exactly
/// where it was, now filed under a ticket for somebody else.
///
/// # Errors
///
/// `Internal` when settings cannot be written.
pub(super) async fn remember(
    core: &AppCore,
    api_key: String,
    ticket: String,
) -> Result<(), GuiError> {
    core.settings()
        .update(SettingsUpdate {
            remote_pairing: Some(Some(RemotePairing { ticket, api_key })),
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
) -> Result<(), GuiError> {
    remember(core, api_key, ticket).await.map_err(spent_code)
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
         cannot be retried with it; run `gglib remote enable` on the other machine for a new one"
    ))
}
