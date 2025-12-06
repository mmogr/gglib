//! Download events - discriminated union for all download state changes.
//!
//! This is a shim that re-exports types from `gglib_core::download`.

// Re-export all event types from gglib-core
pub use gglib_core::download::{DownloadEvent, DownloadStatus, DownloadSummary};
