//! Progress tracking and throttling for downloads.

mod emitter;
mod throttle;

pub use emitter::{build_queue_snapshot, build_queued_summary, ProgressContext};
pub use throttle::ProgressThrottle;