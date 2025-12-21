//! Graceful process shutdown for llama-server instances.
//!
//! Provides two shutdown strategies:
//! - `shutdown_child`: For running processes with a `Child` handle (includes reaping)
//! - `kill_pid`: For orphaned processes from crashes (no reaping, PID-only)

mod child;
mod pid;

pub use child::shutdown_child;
pub use pid::kill_pid;
