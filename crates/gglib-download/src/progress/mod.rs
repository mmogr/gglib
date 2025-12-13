//! Progress tracking and throttling.
//!
//! This module handles progress aggregation and rate-limiting for download
//! progress events.

// TODO(#221): Remove after Phase 2.5 completes
#![allow(dead_code, unused_imports)]

mod throttle;
mod types;

pub use throttle::ProgressThrottle;
pub use types::{ProgressDelta, ProgressSnapshot};
