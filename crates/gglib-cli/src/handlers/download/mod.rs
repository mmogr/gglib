#![doc = include_str!("README.md")]

//! Download command handlers.
//!
//! This module contains handlers for download-related CLI commands.
//! Uses gglib-download's cli_exec module for actual download execution,
//! then registers models in the database via CliContext.

mod browse;
mod check_updates;
mod exec;
mod search;
mod update_model;

pub use browse::execute as browse;
pub use check_updates::execute as check_updates;
pub use exec::{DownloadArgs, execute as download};
pub use search::execute as search;
pub use update_model::execute as update_model;
