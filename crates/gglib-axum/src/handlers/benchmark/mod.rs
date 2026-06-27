#![doc = include_str!("README.md")]
// MIGRATION: content extracted to README.md — remove this //! block after review
//! Benchmark HTTP handlers.
//!
//! Exposes SSE streaming endpoints for compare and perf runs, and REST
//! endpoints for querying the benchmark run history.

pub mod compare;
pub mod history;
pub mod perf;
