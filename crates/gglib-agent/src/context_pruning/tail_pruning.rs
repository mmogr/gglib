//! Pass 2 of the pruning algorithm: emergency tail-prune.
//!
//! Keeps all [`AgentMessage::System`] messages (hoisted to the front) and the
//! last [`AgentConfig::prune_keep_tail_messages`] non-system messages.
//!
//! Called by [`super::prune_for_budget`] when Pass 1 alone was insufficient.
//! See the `# Warning` note on that function for the System message reordering
//! behaviour.

use gglib_core::{AgentConfig, AgentMessage};
use tracing::warn;

/// Emergency tail-prune: keep all `System` messages (hoisted to front) and the
/// last [`AgentConfig::prune_keep_tail_messages`] non-system messages.
pub(super) fn prune_tail(
    messages: Vec<AgentMessage>,
    config: &AgentConfig,
) -> Vec<AgentMessage> {
    let before_count = messages.len();
    let (system, non_system): (Vec<AgentMessage>, Vec<AgentMessage>) = messages
        .into_iter()
        .partition(|m| matches!(m, AgentMessage::System { .. }));

    let tail_start = non_system
        .len()
        .saturating_sub(config.prune_keep_tail_messages);
    let result: Vec<AgentMessage> = system
        .into_iter()
        .chain(non_system.into_iter().skip(tail_start))
        .collect();

    warn!(
        before = before_count,
        after = result.len(),
        dropped = before_count.saturating_sub(result.len()),
        "context-pruning Pass 2 (emergency tail-prune) triggered: \
         System messages hoisted to front, oldest non-system messages dropped"
    );

    result
}
