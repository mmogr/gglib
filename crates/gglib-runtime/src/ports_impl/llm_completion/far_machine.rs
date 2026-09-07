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
//! Which machine was asked is knowable only where the upstream was chosen.
//! Nothing downstream can recover it: by the time an answer comes back, the
//! adapter holds a URL that is loopback either way.

/// The far machine an agent turn is being sent to, and the key it demands.
///
/// Built by whichever surface resolved the remote upstream — the CLI's
/// `--remote` path or the Axum handler's — and carried down to the completion
/// adapter, which sends the key.
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
