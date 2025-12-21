//! Progress reporting abstraction for llama.cpp operations.
//!
//! This module provides a trait-based progress reporting system that allows
//! CLI and GUI adapters to receive progress updates without coupling to
//! specific UI implementations.
//!
//! # Feature Flags
//!
//! - `cli`: Enables `CliProgress` which uses `indicatif` for terminal progress bars.
//!   Without this feature, only `NoopProgress` is available.

/// Trait for receiving progress updates during long-running operations.
///
/// Implementors can display progress bars, update UI elements, or simply
/// ignore the updates (`NoopProgress`).
pub trait ProgressReporter: Send + Sync {
    /// Called when a download/operation starts.
    ///
    /// # Arguments
    /// * `message` - Description of what's starting (e.g., "Downloading llama.cpp")
    /// * `total` - Total size/steps if known
    fn start(&self, message: &str, total: Option<u64>);

    /// Called to update progress.
    ///
    /// # Arguments
    /// * `current` - Current progress (bytes downloaded, steps completed, etc.)
    /// * `total` - Total if known (may differ from start if discovered later)
    fn update(&self, current: u64, total: Option<u64>);

    /// Called to log a message during the operation.
    fn message(&self, msg: &str);

    /// Called when the operation completes successfully.
    fn finish(&self, message: &str);

    /// Called when the operation fails.
    fn finish_with_error(&self, message: &str);
}

/// A no-op progress reporter that ignores all updates.
///
/// Use this when progress reporting is not needed (e.g., in tests or
/// when running without a UI).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopProgress;

impl ProgressReporter for NoopProgress {
    fn start(&self, _message: &str, _total: Option<u64>) {}
    fn update(&self, _current: u64, _total: Option<u64>) {}
    fn message(&self, _msg: &str) {}
    fn finish(&self, _message: &str) {}
    fn finish_with_error(&self, _message: &str) {}
}

/// CLI progress reporter using indicatif for terminal progress bars.
///
/// This is only available with the `cli` feature flag.
#[cfg(feature = "cli")]
pub mod cli_progress {
    use super::ProgressReporter;
    use indicatif::{ProgressBar, ProgressStyle};
    use std::sync::Mutex;

    /// CLI progress reporter with terminal progress bars.
    pub struct CliProgress {
        bar: Mutex<Option<ProgressBar>>,
    }

    impl CliProgress {
        /// Create a new CLI progress reporter.
        pub fn new() -> Self {
            Self {
                bar: Mutex::new(None),
            }
        }

        /// Create a download-style progress bar.
        fn create_download_bar(total: u64) -> ProgressBar {
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
                    .unwrap()
                    .progress_chars("█▓░"),
            );
            pb
        }

        /// Create a spinner for indeterminate progress.
        fn create_spinner() -> ProgressBar {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} [{elapsed_precise}] {msg}")
                    .unwrap(),
            );
            pb
        }
    }

    impl Default for CliProgress {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ProgressReporter for CliProgress {
        fn start(&self, message: &str, total: Option<u64>) {
            let pb = match total {
                Some(t) if t > 0 => Self::create_download_bar(t),
                _ => Self::create_spinner(),
            };
            pb.set_message(message.to_string());

            let mut guard = self.bar.lock().unwrap();
            *guard = Some(pb);
        }

        fn update(&self, current: u64, total: Option<u64>) {
            let guard = self.bar.lock().unwrap();
            if let Some(ref pb) = *guard {
                if let Some(t) = total {
                    pb.set_length(t);
                }
                pb.set_position(current);
            }
        }

        fn message(&self, msg: &str) {
            let guard = self.bar.lock().unwrap();
            if let Some(ref pb) = *guard {
                pb.println(msg);
            } else {
                println!("{}", msg);
            }
        }

        fn finish(&self, message: &str) {
            let mut guard = self.bar.lock().unwrap();
            if let Some(pb) = guard.take() {
                pb.finish_with_message(message.to_string());
            }
        }

        fn finish_with_error(&self, message: &str) {
            let mut guard = self.bar.lock().unwrap();
            if let Some(pb) = guard.take() {
                pb.abandon_with_message(message.to_string());
            }
        }
    }
}

#[cfg(feature = "cli")]
pub use cli_progress::CliProgress;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_progress_does_not_panic() {
        let progress = NoopProgress;
        progress.start("Test", Some(100));
        progress.update(50, Some(100));
        progress.message("Hello");
        progress.finish("Done");
    }
}
