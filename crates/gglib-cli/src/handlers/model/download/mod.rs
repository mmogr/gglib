#![doc = include_str!("README.md")]

mod board;
mod browse;
mod check_updates;
mod companions;
mod exec;
#[allow(
    clippy::manual_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod interactive;
mod monitor;
mod remote;
mod search;
#[cfg(test)]
mod test_hub;
mod update_model;

pub(crate) use browse::execute as browse;
pub(crate) use check_updates::execute as check_updates;
pub(crate) use exec::{DownloadArgs, execute as download};
// Re-exported for `gglib up`, which queues one model and then needs exactly
// this rendering and completion behaviour. A second monitor would be a second
// set of progress-bar and TTY bugs.
pub(crate) use interactive::run_interactive_monitor;
// For `gglib model repair`, whose download is the daemon's as a queued one's
// is, and is watched to its end the same way.
pub(in crate::handlers::model) use remote::monitor_repair;
pub(crate) use search::{execute as search, hub_kind};
pub(crate) use update_model::execute as update_model;
