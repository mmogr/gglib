//! The endpoint a chat turn dials: gglib's own proxy, on this machine or on
//! a paired one.
//!
//! Both are one shape: where the endpoint is, the key it demands, and, for a
//! paired machine, the name it goes by. `name` is what tells them apart:
//! `None` is this machine's endpoint, `Some` is another machine's, reached
//! through the remote tunnel's loopback port (ADR 0012), whose listener
//! injects no credential, so this side sends the key.
//!
//! The name exists because of what a refused key means. A paired machine
//! stops admitting the key stored at pairing time when it stops trusting
//! this device or rotates its `proxy_api_key`, and the refusal that comes
//! back names no machine and offers no remedy. The classifier cannot supply
//! either, because the one signal it has does not separate the cases: this
//! machine's *own* proxy answers a bad key with the same `invalid_api_key`
//! code, on a path where "pair again with the other machine" would be
//! nonsense. So the door that was dialled says which sentence is right
//! ([`Door::refusal`]), and that is knowable only where the door was opened.

use std::fmt;

use gglib_core::services::SETTINGS_CACHE_TTL;

/// The endpoint a chat turn dials, and the key it demands.
///
/// Built by whichever surface resolved it: the far proxy for a paired
/// machine, `ProxyOps::door` for this machine's endpoint. Carried down to the
/// completion adapter, which sends the key and, when the endpoint refuses
/// it, says so in this door's words.
///
/// Its `Debug` never prints the key, so a door formatted into a log line by
/// accident carries no credential.
#[derive(Clone)]
pub struct Door {
    /// The endpoint's root, `http://<host>:<port>`, without the `/v1`: a
    /// caller appends the path it posts to.
    pub root_url: String,
    /// The key the endpoint demands, sent as `Authorization: Bearer …` and
    /// read nowhere else. `None` while the endpoint demands none.
    pub key: Option<String>,
    /// The paired machine's name, as every remote surface shows it (its own
    /// name, or the fallback for one that gave none). `None` for this
    /// machine's endpoint. For a sentence only; nothing compares it.
    pub name: Option<String>,
}

/// Never the key.
impl fmt::Debug for Door {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Door")
            .field("root_url", &self.root_url)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The `error.code` for a bearer that was not accepted.
///
/// On a paired machine two authors write it, and the conclusion survives
/// both. modelpipe's serve-side edge writes it before contacting any backend
/// (`exchange.rs` checks the credential first); the far machine's own gglib
/// proxy writes it if the edge admitted the request and it did not. Both
/// enforce that machine's `proxy_api_key` — the tunnel is handed it, and
/// `remote::rotation` follows a change of it into the running listener — so
/// either way the answer means the far side did not accept the key this side
/// sent, which is the only thing the message claims.
///
/// The two are not perfectly in step: `rotation` polls, so for one settings
/// cache TTL the edge and the proxy can be enforcing different values, and a
/// key that *is* that machine's current one can be refused by whichever of
/// them has not caught up. The message survives that too, but only because it
/// claims no more than the refusal does — it says the key was not accepted,
/// not that it was wrong — and re-pairing inside that window is harmless.
///
/// This machine's proxy emits the identical code for its *own* bad keys, so
/// the sentence is the door's: a paired machine's names it, this machine's
/// sends nobody to re-pair.
const INVALID_API_KEY: &str = "invalid_api_key";

impl Door {
    /// Whether this door is a paired machine's.
    #[must_use]
    pub const fn is_far(&self) -> bool {
        self.name.is_some()
    }

    /// What a person should read when this door refused the turn, or `None`
    /// when the refusal is not one this side can explain better than the
    /// classifier already did.
    ///
    /// Only a refused key qualifies today. Every other way a turn can fail is
    /// about the tunnel or about the model server behind the endpoint, and
    /// for those the door adds nothing the rendered reason does not already
    /// carry. Matching on `code` and never on message text is the same rule
    /// the classifier is held to.
    #[must_use]
    pub fn refusal(&self, code: Option<&str>) -> Option<String> {
        (code == Some(INVALID_API_KEY)).then(|| self.key_refused())
    }

    /// What a person reads when this door refused the key it was dialled
    /// with.
    ///
    /// A paired machine's sentence says the stored key is not admitted, and
    /// deliberately stops short of saying the machine rotated it: the device
    /// may have stopped being trusted, or a rotation there has not reached
    /// the tunnel yet, and `remote::connect` will dial a bare ticket for a
    /// *different* machine while leaving an earlier pairing's key in place.
    /// The remedy is the same in each case.
    ///
    /// This machine's door is dialled with the key its endpoint's bearer
    /// policy reads, and the endpoint reads it through a settings cache, so
    /// the one way it is refused is a key changed within that cache's window:
    /// sending again once the window has passed is the remedy.
    #[must_use]
    pub fn key_refused(&self) -> String {
        self.name.as_deref().map_or_else(
            || {
                format!(
                    "this machine's endpoint did not admit its own key — the key was changed a \
                     moment ago, and the endpoint takes up to {} seconds to pick up a new one. \
                     Send again in a few seconds",
                    SETTINGS_CACHE_TTL.as_secs()
                )
            },
            |name| {
                format!(
                    "{name} is not admitting this device's key — it has either stopped trusting \
                     this device, or a key rotation there is still reaching the tunnel, which \
                     clears itself within a few seconds. If waiting does not fix it, pair again \
                     with a fresh `gglib remote invite` there"
                )
            },
        )
    }
}

#[cfg(test)]
#[path = "door_tests.rs"]
mod door_tests;
