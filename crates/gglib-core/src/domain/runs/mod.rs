#![doc = include_str!("README.md")]

mod id;
mod wire;

pub use id::{RUN_ID_MAX, is_run_id, new_run_id};
pub use wire::{RunError, RunInfo, RunKind, RunList, RunStatus};

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
