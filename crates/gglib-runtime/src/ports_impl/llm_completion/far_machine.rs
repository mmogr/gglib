//! The machine on the other end of the tunnel, when there is one.
//!
//! An agent turn goes to one of two places: a llama-server on this machine's
//! loopback, which asks for no credential and has no name worth printing, or
//! the tunnel's loopback port, which is *another* machine's proxy (ADR 0012).
//! Only the second has a key, and only the second has an identity — so the two
//! travel together in one value rather than as two optionals that could
//! disagree. `None` means local; `Some` means remote, with the key and the
//! name of whoever demands it.
//!
//! This exists because of what the second case can fail with. A key this
//! machine stored at pairing time stops working the moment the far machine
//! rotates its `proxy_api_key`, and the refusal that comes back names no
//! machine and offers no remedy. The classifier cannot supply either, because
//! the one signal it has does not separate the cases: this machine's *own*
//! proxy answers a bad key with the same `invalid_api_key` code, on a path
//! where "pair again with the other machine" would be nonsense. The rest of
//! that body differs — ours names a `type` and says something else — but the
//! code, which is the only part anything here may read, collides exactly.
//!
//! Which machine was asked is knowable only where the upstream was chosen,
//! which is where one of these is built.

/// The far machine an agent turn is being sent to, and the key it demands.
///
/// Built by whichever surface resolved the remote upstream — the CLI's
/// `--remote` path or the Axum handler's — and carried down to the completion
/// adapter, which sends the key and, when the far machine refuses it, says so
/// by name.
///
/// No `Debug`, deliberately, for the same reason
/// [`LlmCompletionAdapter`](super::LlmCompletionAdapter) derives none: this
/// holds a live credential, and a struct that cannot be formatted cannot be
/// formatted into a log line by accident.
#[derive(Clone)]
pub struct FarMachine {
    /// The key this machine received when it paired with that one.
    ///
    /// Sent as `Authorization: Bearer …`, and read nowhere else.
    pub key: String,
    /// The paired ticket's fingerprint.
    ///
    /// The only name this side has for the other side, and the one every
    /// other remote surface already prints: `gglib remote status`, the CLI's
    /// pre-turn banner and the connect confirmation all name the far machine
    /// this way, so a message built from it names something the user has
    /// already been shown.
    pub fingerprint: String,
}

/// The `error.code` for a bearer that was not accepted.
///
/// Two authors write it on a remote turn, and the conclusion survives both.
/// modelpipe's serve-side edge writes it before contacting any backend
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
/// This machine's proxy emits the identical code for its *own* bad keys,
/// which is why nothing may act on this without first knowing a far machine
/// was involved at all.
const INVALID_API_KEY: &str = "invalid_api_key";

impl FarMachine {
    /// What a person should read when this machine refused the turn, or
    /// `None` when the refusal is not one this side can explain better than
    /// the classifier already did.
    ///
    /// Only a refused key qualifies today. Every other way a remote turn can
    /// fail is about the tunnel or about the model server behind it, and for
    /// those the far machine's identity adds nothing the rendered reason does
    /// not already carry. Matching on `code` and never on message text is the
    /// same rule the classifier is held to.
    ///
    /// The wording says the stored key is not that machine's current one, and
    /// deliberately stops short of saying the machine rotated it. Rotation is
    /// the common cause but not the only one: `remote::connect` will dial a
    /// bare ticket for a *different* machine while leaving an earlier
    /// pairing's `remote_api_key` in place, so the key can be refused by a
    /// machine whose own key never changed. "Not that machine's key" covers
    /// both, and the remedy is the same either way.
    pub(super) fn refusal(&self, code: Option<&str>) -> Option<String> {
        (code == Some(INVALID_API_KEY)).then(|| {
            format!(
                "the remote machine {} refused the stored key — it is not that machine's current \
                 API key; pair again with a fresh `gglib remote enable` there",
                self.fingerprint
            )
        })
    }
}
