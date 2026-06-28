#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
//! Progress tracking and throttling.
//!
//! This module handles progress aggregation and rate-limiting for download
//! progress events.

mod throttle;

pub use throttle::ProgressThrottle;
