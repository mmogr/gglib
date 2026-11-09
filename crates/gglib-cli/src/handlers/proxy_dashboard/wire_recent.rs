//! The recent-requests rows of the dashboard frame, as this dashboard reads
//! them: the model, when, what it wrote and how fast, and what gglib did, in
//! the daemon's words.

use serde::Deserialize;

/// One recent request: a mirror of the proxy's `ContextSnapshot`, narrowed to
/// what this section prints.
#[derive(Debug, Deserialize)]
pub(super) struct RecentRequest {
    pub(super) model_name: String,
    pub(super) recorded_at_secs: u64,
    /// Tokens the request generated; `None` until its turn is over, and on
    /// a proxy older than the field.
    #[serde(default)]
    pub(super) completion_tokens: Option<u32>,
    /// How fast it wrote, the proxy's figure; `None` with no time writing,
    /// and on a proxy older than the field.
    #[serde(default)]
    pub(super) tokens_per_second: Option<f64>,
    /// What gglib did to it, each with its word. `default` on a proxy older
    /// than the field.
    #[serde(default)]
    pub(super) interventions: Vec<Intervention>,
}

/// One thing gglib did, as the word the proxy chose for it. The kind rides
/// beside it on the wire and is not read here: the word is the whole fact
/// this dashboard shows.
#[derive(Debug, Deserialize)]
pub(super) struct Intervention {
    pub(super) word: String,
}
