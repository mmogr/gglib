#![doc = include_str!("README.md")]

mod wire;

pub use wire::{RunError, RunInfo, RunKind, RunList, RunStatus};

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
