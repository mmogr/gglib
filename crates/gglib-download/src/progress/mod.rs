//! Progress tracking and throttling.
//!
//! This module handles progress aggregation and rate-limiting for download
//! progress events.

mod throttle;

pub use throttle::ProgressThrottle;
