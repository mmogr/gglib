//! What `gglib chat --thinking` takes: a chat's Thinking choice, in the
//! words of the chat page's switch.
//!
//! A `ValueEnum` mirror of the domain's [`Thinking`], kept beside the command
//! it parameterises as `model_sort`'s enums are. A mirror because `gglib-core`
//! may not depend on clap (`scripts/check_boundaries.sh`), and because the
//! switch reads `on` where a turn says `default`. The `From` below is the
//! only place the two are mapped.

use clap::ValueEnum;
use gglib_core::domain::Thinking;

/// A chat's Thinking choice, as `--thinking` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ThinkingArg {
    /// The model thinks, within --reasoning-budget-tokens when one is typed
    On,
    /// Turns run with a thinking budget of 0
    Off,
}

impl From<ThinkingArg> for Thinking {
    fn from(arg: ThinkingArg) -> Self {
        match arg {
            ThinkingArg::On => Self::Default,
            ThinkingArg::Off => Self::Off,
        }
    }
}
