#![doc = include_str!("README.md")]
pub mod daemon;
pub mod hf;

// Re-export for convenience
pub use hf::*;
// `daemon` stays qualified: its names (HEALTH_PATH, MODELS_LIST_PATH) are
// generic enough that a glob would make the call site ambiguous about which
// surface's contract it means.
