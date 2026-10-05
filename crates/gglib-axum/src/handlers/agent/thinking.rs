//! The one rule for a chat's Thinking choice, read at every door a turn
//! comes through: a paired device's turn, the page's turn on a far machine
//! (which arrives there as a device's), and this machine's own run.
//!
//! A turn says `off`, `default` or nothing. `off` runs it with a thinking
//! budget of `0` and the chat remembers; `default` runs it with the
//! request's own budget and the chat forgets; nothing runs it as the chat
//! remembers, and a remembered `off` beats the request's own budget. The
//! request's own budget and effort level are that request's: they are never
//! remembered.

use gglib_core::domain::Thinking;

/// The thinking budget that switches a model's thinking off.
const OFF_BUDGET: i32 = 0;

/// What a chat is to remember once a run starts: `None` writes nothing,
/// `Some(choice)` sets what it remembers to `choice` (`Some(None)` forgets).
pub(super) type Remember = Option<Option<Thinking>>;

/// A turn's Thinking choice, settled against its chat's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Settled {
    /// The thinking budget the run uses, as `reasoning_budget_tokens`.
    pub(super) budget: Option<i32>,
    /// What the chat is to remember, written when the run is launched.
    pub(super) remember: Remember,
}

/// Settle what a turn `said` against what its chat `remembered` and the
/// request's `own_budget`, which a device's turn never has.
///
/// Only `off` is ever remembered, so a write is asked for only when it
/// changes that: `off` on a chat that does not already remember it, and
/// `default` on one that remembers anything.
pub(super) fn settle(
    said: Option<Thinking>,
    remembered: Option<Thinking>,
    own_budget: Option<i32>,
) -> Settled {
    let off = Some(Thinking::Off);
    match said {
        Some(Thinking::Off) => Settled {
            budget: Some(OFF_BUDGET),
            remember: (remembered != off).then_some(off),
        },
        Some(Thinking::Default) => Settled {
            budget: own_budget,
            remember: remembered.is_some().then_some(None),
        },
        None if remembered == off => Settled {
            budget: Some(OFF_BUDGET),
            remember: None,
        },
        None => Settled {
            budget: own_budget,
            remember: None,
        },
    }
}

#[cfg(test)]
#[path = "thinking_tests.rs"]
mod thinking_tests;
