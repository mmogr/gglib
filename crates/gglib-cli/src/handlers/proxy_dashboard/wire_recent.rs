//! The recent-requests rows of the dashboard frame, as this dashboard reads
//! them: the model, when, and what gglib did, in the daemon's words.

use serde::Deserialize;

/// One recent request: a mirror of the proxy's `ContextSnapshot`, narrowed to
/// what this section prints.
#[derive(Debug, Deserialize)]
pub(super) struct RecentRequest {
    pub(super) model_name: String,
    pub(super) recorded_at_secs: u64,
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
