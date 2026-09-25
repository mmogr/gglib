//! The record settings keep of the machine this one paired with.
//!
//! Reading it, writing it, what a write that fails after the code has been
//! spent has to say, and — in [`settle`] — which of those a dial that has
//! come up owes. A sibling rather than more of `connect.rs`, which sits a
//! handful of lines under the file-size budget — and because `mod.rs`'s
//! status surface asks the same question of the same record as `connect`
//! does, and asking it in two places is how the two drift.

use gglib_core::services::AppCore;
use gglib_core::{RemotePairing, Settings, validate_settings};
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
/// reason the connect watcher takes its statuses as one: a pairing needs a
/// live tunnel, so a decision reachable only *through* `modelpipe::connect`
/// — an iroh endpoint and a peer that answers — is a comment with an `await`
/// in it rather than something a test can drive. Both arms are decisions
/// worth driving: what a redeemed key replaces and what it keeps, and on the
/// codeless arm, that a dial to the machine already recorded writes nothing
/// at all.
///
/// `code` is whatever `redeem` spends, and a secret either way: `dial` passes
/// the key `modelpipe::pair` already bought and redeems it by handing it
/// straight back, so nothing here may log or format it.
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
            store_redeemed(core, key, ticket, port).await?;
            Ok(true)
        }
        None => {
            // The caller refused a codeless dial with no key for this
            // machine, so `held` is `Some`. Filing the ticket just dialled
            // beside that machine's key is how the record follows a machine
            // that moved: same identity, new addresses, same key — and the
            // port just bound, which is how the address a client was
            // configured against stays the address. It is the only write on
            // this arm, so a dial to the machine already recorded, on the
            // port already recorded, touches nothing.
            if held.is_some_and(|held| held.ticket != ticket.to_string() || held.port != Some(port))
            {
                remember(core, |stored| follow(stored, ticket, port)).await?;
            }
            Ok(false)
        }
    }
}

/// Change the stored pairing as it stands when the write lands.
///
/// Not as `connect` read it before the dial. A `--remote` turn in a terminal
/// remembers its model on the same record while a dial is under way, and the
/// record can be cleared meanwhile; one rebuilt from the earlier read would
/// undo either. So `change` is handed the record inside one
/// [`modify`](gglib_core::ports::SettingsRepository::modify), and each caller
/// decides from that record what to keep.
///
/// # Errors
///
/// `Internal` when settings cannot be written, including when the result
/// does not validate.
pub(super) async fn remember(
    core: &AppCore,
    change: impl Fn(&mut Option<RemotePairing>) + Send + Sync,
) -> Result<(), GuiError> {
    core.settings()
        .repo()
        .modify(&|settings: &mut Settings| {
            change(&mut settings.remote_pairing);
            validate_settings(settings)
        })
        .await
        .map(drop)
        .map_err(|e| GuiError::Internal(format!("could not store the pairing: {e}")))
}

/// The ticket and the port a codeless dial used, filed on the record of the
/// machine it reached.
///
/// Only on that machine's record. Another machine's, paired while this dial
/// was under way, keeps its own ticket beside its own key, and a record
/// forgotten meanwhile stays forgotten.
fn follow(stored: &mut Option<RemotePairing>, ticket: &Ticket, port: u16) {
    if let Some(stored) = stored
        .as_mut()
        .filter(|stored| names_the_same_machine(stored, ticket))
    {
        stored.ticket = ticket.to_string();
        stored.port = Some(port);
    }
}

/// Store a pairing whose code has just been redeemed.
///
/// The ticket, the key and the port are this dial's. The model is kept when
/// the record already names the machine just paired with, since it is a
/// name in that machine's catalogue, whenever it was remembered; a pairing
/// with any other machine starts with nothing remembered.
///
/// Its failure says more than [`remember`]'s, and that is the point: by the
/// time this runs the code is gone, so "could not store the pairing" is the
/// one reading a person must not be left with.
///
/// # Errors
///
/// `Internal` when settings cannot be written, in the wording below.
pub(super) async fn store_redeemed(
    core: &AppCore,
    api_key: String,
    ticket: &Ticket,
    port: u16,
) -> Result<(), GuiError> {
    remember(core, |stored| {
        let default_model = stored
            .take()
            .filter(|stored| names_the_same_machine(stored, ticket))
            .and_then(|stored| stored.default_model);
        *stored = Some(RemotePairing {
            ticket: ticket.to_string(),
            api_key: api_key.clone(),
            default_model,
            port: Some(port),
        });
    })
    .await
    .map_err(spent_code)
}

/// A failure to store a pairing whose code has already been spent.
///
/// Worth its own sentence because the ordinary reading of "could not store"
/// is "try again", and trying again cannot work: `modelpipe::pair` spent the
/// code at the far machine's tunnel edge, which answers each code once, so
/// the next `connect` meets a
/// refusal it reports as a refused pairing code. The only way forward
/// is a fresh `gglib remote invite` there, and saying so is the difference between
/// walking to the other machine once and doing it after an hour of retries.
///
/// It does not get the key back. Nothing at this layer can: the key exists
/// only in the response that was just read, and the store that would have
/// kept it is the thing that failed.
fn spent_code(e: GuiError) -> GuiError {
    GuiError::Internal(format!(
        "{e} — the pairing code was already spent redeeming this key, so `gglib remote join` \
         cannot be retried with it; run `gglib remote invite` on the other machine for a \
         new one"
    ))
}

#[cfg(test)]
#[path = "stored_pairing_tests.rs"]
mod stored_pairing_tests;

#[cfg(test)]
#[path = "stored_pairing_redial_tests.rs"]
mod stored_pairing_redial_tests;
