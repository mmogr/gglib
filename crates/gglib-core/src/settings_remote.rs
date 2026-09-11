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

/// The machine `gglib remote join` paired with, and the key that machine
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
/// Written only by `gglib remote join`: there is no CLI flag and no GUI
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
    /// takes [`Self::api_key`]. It used to go stale every time that machine
    /// ran `enable`, because each one minted a fresh identity; identities
    /// last now, so a ticket stays good across the far machine's restarts
    /// and a device pairs once. A later dial to the *same* machine at a new
    /// address still replaces this and keeps the key, which is what makes
    /// the ticket the mutable half.
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

    /// The model a `--remote` turn is for when the command line names none:
    /// the one this machine last asked that machine for.
    ///
    /// Remembered rather than configured — there is no flag and no setting
    /// to type it into — because the alternative was naming the model on
    /// every turn, and the model a person asks a machine for is the one
    /// they asked it for last time. Per pairing, not global: it is a name
    /// in *that* machine's catalogue, and it goes with the record when the
    /// pairing does. `#[serde(default)]` so a record written before the
    /// field existed loads as nothing remembered yet.
    #[serde(default)]
    pub default_model: Option<String>,

    /// The loopback port the paired machine was last reachable at here,
    /// tried first next time so the address a client was configured
    /// against stays the address.
    ///
    /// Stable rather than fixed: a port can be taken by something else
    /// between two sessions, and `connect` then binds the next free one,
    /// says so, and remembers *that*. `--port` pins it, and is remembered
    /// the same way. `#[serde(default)]` for the reason the field above
    /// gives.
    #[serde(default)]
    pub port: Option<u16>,
}

/// How this machine was told to put its proxy on the tunnel, kept so a
/// restart arms it the same way.
///
/// Its companion is [`Settings::remote_enabled`], the switch `gglib remote
/// enable` and `disable` set and the one thing the daemon reads at startup
/// to decide whether to bring the tunnel back up. That field mirrors
/// `proxy_autostart` deliberately — same shape, same tri-state, same reason:
/// a machine you reach from elsewhere is not a feature you want to remember
/// to switch on after every reboot. Neither is ever typed; there is no flag
/// for either, because the flag *is* the command.
///
/// This record is the other half — the flags that `enable` was given, so a
/// resumed tunnel is armed the way it was enabled. Without it a restart
/// would quietly change behaviour, and `--allow-mcp` is a deliberate
/// decision on one machine: silently forgetting it is the failure that
/// matters, not the noise of remembering. Absent means never enabled.
///
/// The flags `gglib remote enable` accepts, and nothing else: this is a
/// record of a decision, not a place to configure one. There is no CLI path
/// that writes it directly and no GUI field for it — `enable` writes it
/// whole, the way `remote_pairing` is written whole, because the flags were
/// one decision taken at one moment and a half-applied set of them is not a
/// state anybody asked for.
///
/// Persisted as one `settings_kv` row holding a JSON object, camelCase
/// inside, matching `remote_pairing`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServe {
    /// Whether requests arriving through the tunnel may reach `/mcp`.
    ///
    /// Off unless asked for, and the one flag here with teeth: `invoke_tool`
    /// starts the MCP servers configured on this machine, so a leaked key
    /// with a shell server configured is remote code execution. Surviving a
    /// restart is the point — silently dropping it would be a security
    /// posture that changes when nobody is looking.
    #[serde(default)]
    pub allow_mcp: bool,

    /// A self-hosted relay URL, or `None` for n0's public relays.
    #[serde(default)]
    pub relay: Option<String>,

    /// Whether to publish to, and resolve through, n0's discovery service.
    ///
    /// `true` unless `--no-discovery` was given. With a lasting identity
    /// this matters more than it did: the ticket now outlives the session,
    /// so a ticket minted without discovery keeps only the paths it was
    /// minted with and stops resolving the moment this machine changes
    /// network — for good, not until the next `enable`.
    #[serde(default = "default_true")]
    pub discovery: bool,
}

/// One device this machine has issued a key to.
///
/// The roster, and only the roster: **no key field**. A device's key is a
/// secret and lives in the `0600` file beside the endpoint identity, not
/// here — `gglib config settings show` prints `proxy_api_key` unmasked by
/// design, and that output gets pasted into bug reports. One shared key
/// there was a known cost; every device key there would quietly undo what
/// per-device revocation is for.
///
/// `id` is what modelpipe is told, and it travels to the backend as
/// `X-Modelpipe-Device` on every request that device makes, so it is
/// generated from the CSPRNG rather than derived from the key: an
/// identifier that falls out of a live credential is needless coupling at
/// best. `label` is for a person to read and is sent nowhere, because
/// modelpipe's names are `[A-Za-z0-9._-]{1,64}` and "Matt's iPhone" is not
/// one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// The name the tunnel edge holds this device's token under.
    pub id: String,

    /// What a person calls it, when the device said so at join.
    #[serde(default)]
    pub label: Option<String>,

    /// Unix milliseconds at which this device's invite was minted.
    ///
    /// Not when it redeemed one: the row and the key are written before the
    /// code is shown, so that a device cannot end up holding a key this side
    /// has no record of. An invite nobody redeems therefore leaves a row
    /// behind, which is why it is listed rather than swept on a timer.
    pub joined_at: i64,

    /// Unix milliseconds at which a device redeemed this row's invite, or
    /// `None` if none ever has.
    ///
    /// The counterpart to [`joined_at`](Self::joined_at), which is when the
    /// invite was *minted*: a row with a `joined_at` and no `redeemed_at` is
    /// an invite nobody took, and is listed as such rather than swept on a
    /// timer. Written by the roster's writer rather than on the request
    /// path, so it is advisory in the same way a label is — which is why a
    /// row is only ever called never-joined when `last_seen` is empty too.
    /// A device that has made a request has plainly joined, whatever this
    /// says.
    #[serde(default)]
    pub redeemed_at: Option<i64>,

    /// Unix milliseconds of the last request that arrived bearing this
    /// device's token, or `None` if none has since the daemon started.
    ///
    /// Advisory, like the tunnelled request counter: it is written from a
    /// background task rather than the request path, and a local process
    /// that forges the marker headers can move it. Nothing is granted on
    /// it — it exists so a person deciding what to `forget` can see which
    /// row is still in use.
    #[serde(default)]
    pub last_seen: Option<i64>,
}

/// `serde(default)` for a field whose absence means yes.
const fn default_true() -> bool {
    true
}

impl Settings {
    /// Apply the remote half of `other`: every remote field, and only those.
    pub(super) fn merge_remote(&mut self, other: &SettingsUpdate) {
        if let Some(ref v) = other.remote_pairing {
            self.remote_pairing.clone_from(v);
        }
        if let Some(v) = other.remote_enabled {
            self.remote_enabled = v;
        }
        if let Some(ref v) = other.remote_serve {
            self.remote_serve.clone_from(v);
        }
        if let Some(ref v) = other.remote_devices {
            self.remote_devices.clone_from(v);
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
    // A row whose id is not a name modelpipe will hold is a row that cannot
    // be seeded, and the failure would land at the next `enable` rather than
    // at the write that caused it.
    for device in settings.remote_devices.iter().flatten() {
        if !valid_device_id(&device.id) {
            return Err(SettingsError::InvalidDeviceId(device.id.clone()));
        }
    }
    Ok(())
}

/// modelpipe's rule for a token name, applied before a row is written
/// rather than when the listener refuses it: ASCII letters, digits, `.`,
/// `_` and `-`, one to sixty-four bytes.
fn valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
