//! The remote tunnel's settings (ADR 0012): the connect side's stored
//! pairing as one value, and the remote half of merging and validating.
//!
//! Split out via `#[path]`, the way `settings_validate.rs` is, and for the
//! same reason: `settings.rs` sits exactly on its ratchet baseline, so a type
//! that carries its own doc comment cannot live in it — and neither can the
//! arms that grow with it. `merge` and `validate_settings` each call into
//! here once, so a remote field added later touches `settings.rs` by one
//! line for its declaration and nothing else.

use serde::{Deserialize, Serialize};

use super::{Settings, SettingsError, SettingsUpdate};

/// The machine `gglib remote connect` paired with, and the key that machine
/// issued — one record, because they are one fact.
///
/// They were two settings rows, `remote_last_ticket` and `remote_api_key`,
/// written independently by the same call. A key is issued *by* the machine
/// whose one-time code was redeemed, so it means nothing apart from the
/// ticket that names that machine: a bare-ticket dial to a second machine
/// rewrote the ticket and left the first machine's key sitting beside it,
/// and `gglib remote status` then reported a fully paired connection whose
/// every request came back `401`. Holding the two in one record makes that
/// disagreement unrepresentable rather than merely wrong, and gives the
/// stale-key question — *whose* key is this? — an answer.
///
/// Written only by `gglib remote connect`: there is no CLI flag and no GUI
/// control, and `gglib config settings show` reports the key as held-or-not
/// rather than printing it.
/// Persisted as one `settings_kv` row holding a JSON object, the way
/// `inference_defaults` is — camelCase inside, to match it and so that the
/// CLI's kebab-casing of nested keys reads `remote-pairing.api-key`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemotePairing {
    /// The ticket that machine handed out, in its canonical form.
    ///
    /// An address rather than a credential — reaching the far side still
    /// takes [`Self::api_key`] — and it goes stale the moment that machine
    /// runs `enable` again, because every `enable` mints a fresh identity. A
    /// later dial to the *same* machine at a new address replaces this and
    /// keeps the key, which is what makes the ticket the mutable half.
    pub ticket: String,

    /// That machine's API key: its `proxy_api_key`, received by redeeming
    /// its one-time pairing code through the tunnel.
    ///
    /// Not optional, deliberately. A record that could hold a ticket with no
    /// key is the shape the desync above lived in — the half-write that lost
    /// the binding. A dial to a machine this one holds no key for is refused
    /// before it is made, so there is no state left for such a record to
    /// describe.
    pub api_key: String,
}

impl Settings {
    /// Apply the remote half of `other`: every remote field, and only those.
    pub(super) fn merge_remote(&mut self, other: &SettingsUpdate) {
        if let Some(ref v) = other.remote_pairing {
            self.remote_pairing.clone_from(v);
        }
    }
}

/// The remote half of [`validate_settings`](super::validate_settings).
///
/// The connect side's stored pairing, same rule on each half: a blank is
/// neither a key nor an address, and `connect` reading one would dial
/// nothing with nothing rather than say the pairing is gone. Clearing the
/// record is how a pairing is forgotten.
pub(super) fn validate_remote(settings: &Settings) -> Result<(), SettingsError> {
    if let Some(ref pairing) = settings.remote_pairing {
        if pairing.api_key.trim().is_empty() {
            return Err(SettingsError::BlankRemoteApiKey);
        }
        if pairing.ticket.trim().is_empty() {
            return Err(SettingsError::BlankRemoteTicket);
        }
    }
    Ok(())
}
