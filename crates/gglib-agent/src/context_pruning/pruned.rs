//! [`Pruned`]: a run's messages, kept within the context budget, and how
//! many of them that has cost.

use std::ops::{Deref, DerefMut};

use gglib_core::{AgentConfig, AgentMessage};

use super::prune_for_budget;

/// A run's messages, pruned to the context budget, with a count of the
/// messages pruning has dropped since the run began.
///
/// It reads and writes as the `Vec` it holds. The count only grows: a
/// message dropped before one model call is still missing from the request
/// every later call of the same run answers, so each call's
/// [`TurnUsage`](gglib_core::domain::agent::TurnUsage) reports the total so
/// far, not what the last prune alone dropped.
pub(crate) struct Pruned {
    messages: Vec<AgentMessage>,
    dropped: usize,
}

impl Pruned {
    /// `messages`, pruned to `config`'s budget.
    pub(crate) fn new(messages: Vec<AgentMessage>, config: &AgentConfig) -> Self {
        let mut pruned = Self {
            messages,
            dropped: 0,
        };
        pruned.prune(config);
        pruned
    }

    /// Prune to `config`'s budget again, adding what this drops to the count.
    pub(crate) fn prune(&mut self, config: &AgentConfig) {
        let before = self.messages.len();
        self.messages = prune_for_budget(std::mem::take(&mut self.messages), config);
        self.dropped += before.saturating_sub(self.messages.len());
    }

    /// How many messages have been dropped since the run began.
    pub(crate) const fn dropped(&self) -> usize {
        self.dropped
    }
}

impl Deref for Pruned {
    type Target = Vec<AgentMessage>;

    fn deref(&self) -> &Self::Target {
        &self.messages
    }
}

impl DerefMut for Pruned {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.messages
    }
}

#[cfg(test)]
#[path = "pruned_tests.rs"]
mod tests;
