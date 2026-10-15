#![doc = include_str!("README.md")]

mod handlers;
mod scope;
pub mod sse;
mod turn;

pub(crate) use handlers::{cancel_run, get_run, list_runs, put_run, run_events};

/// The refusal of an agent run whose query says `draw` or `image_model`, at
/// this door and at the daemon's own: never dropped in silence, since a turn
/// that said Draw there would run and draw nothing.
pub const AGENT_DRAW_IN_QUERY: &str = "an agent run says `draw` and `image_model` in its body, \
                                       `\"draw\": true`, not in the query";
