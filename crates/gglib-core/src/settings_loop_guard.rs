//! What the proxy's loop guard does when a replayed history trips it.
//!
//! A `#[path]` sibling of `settings.rs` because the type is a settings value
//! with a wire spelling and a generated TypeScript mirror, and because the
//! precedence rule that reconciles it with the boolean it replaces is a page
//! of argument that belongs beside it rather than in the middle of the
//! `Settings` struct.

use serde::{Deserialize, Serialize};

/// What the loop guard does with a request whose replayed history trips it.
///
/// Replaces the boolean [`proxy_loop_detection`](super::Settings::proxy_loop_detection),
/// which could only say "scan" or "do not scan" and made the scan's only
/// answer a terminal HTTP 400. ADR 0011 records that 400 ending a Copilot
/// session on its sixth turn: an external agentic client has no recovery path
/// from a refusal, and because it replays the whole conversation every turn,
/// the refusal repeats for the rest of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum LoopGuardMode {
    /// Do not scan at all.
    ///
    /// The escape hatch for a client that legitimately repeats identical
    /// tool-call batches with nothing in between, or repeats a response. What
    /// `proxy_loop_detection = Some(false)` meant, and still means.
    Off,
    /// Forward the request, with a note appended to the last message saying
    /// what repeated and how often.
    ///
    /// The default. The model is told what gglib can see and is left to act
    /// on it, which is the one thing a refusal cannot offer a client that has
    /// no recovery path. The cost is that a genuinely runaway client now
    /// spends a generation per stuck turn instead of being stopped at
    /// threshold + 1 — the trade #1052 asks for.
    #[default]
    Note,
    /// Refuse the request with HTTP 400, before any catalog, admission or
    /// model-swap cost.
    ///
    /// What the guard did by default until #1052. Still the right answer for
    /// an operator who would rather a stuck session fail loudly than burn a
    /// shared GPU.
    Refuse,
}

impl LoopGuardMode {
    /// Whether the history is scanned at all under this mode.
    #[must_use]
    pub const fn scans(self) -> bool {
        !matches!(self, Self::Off)
    }
}
