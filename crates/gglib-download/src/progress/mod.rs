//! Progress tracking, throttling, and speed estimation.
//!
//! This module handles progress aggregation, rate-limiting for download
//! progress events, and the shared sliding-window speed estimator used by
//! all display paths.

pub mod rate;
mod throttle;

pub use rate::{SlidingWindowRate, format_eta};
pub use throttle::ProgressThrottle;
