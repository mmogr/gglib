#![doc = include_str!("README.md")]
mod child;
mod pid;

pub use child::shutdown_child;
pub use pid::kill_pid;
