#![doc = include_str!("README.md")]

//! Download command handlers.
//!
//! This module contains handlers for download-related CLI commands.
//! Uses gglib-download's cli_exec module for actual download execution,
//! then registers models in the database via CliContext.

mod browse;
mod check_updates;
mod exec;
mod interactive;
mod remote;
mod search;
mod update_model;

pub(crate) use browse::execute as browse;
pub(crate) use check_updates::execute as check_updates;
pub(crate) use exec::{DownloadArgs, execute as download};
// Re-exported for `gglib up`, which queues one model and then needs exactly
// this rendering and completion behaviour. A second monitor would be a second
// set of progress-bar and TTY bugs.
pub(crate) use interactive::run_interactive_monitor;
pub(crate) use search::execute as search;
pub(crate) use update_model::execute as update_model;
