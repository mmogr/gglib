use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Information about shard progress in a sharded download
#[derive(Clone, Debug, Serialize)]
pub struct ShardProgressInfo {
    /// Current shard index (0-based)
    pub current_shard: usize,
    /// Total number of shards
    pub total_shards: usize,
    /// Filename of the current shard
    pub current_filename: String,
    /// Bytes downloaded for this shard
    pub shard_downloaded: u64,
    /// Total bytes for this shard
    pub shard_total: u64,
    /// Aggregate bytes downloaded across all shards so far
    pub aggregate_downloaded: u64,
    /// Aggregate total bytes across all shards
    pub aggregate_total: u64,
}

/// Shared progress payload for desktop and web download UIs
#[derive(Clone, Debug, Serialize)]
pub struct DownloadProgressEvent {
    pub status: String,
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
    pub speed: f64,
    pub eta: f64,
    pub message: Option<String>,
    /// Position in the download queue (1 = currently downloading, 2+ = waiting)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_position: Option<usize>,
    /// Total number of items in the queue (including current download)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_length: Option<usize>,
    /// Shard progress information for sharded model downloads
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard_progress: Option<ShardProgressInfo>,
}

impl DownloadProgressEvent {
    fn base(model_id: &str, status: &str) -> Self {
        Self {
            status: status.to_string(),
            model_id: model_id.to_string(),
            downloaded: 0,
            total: 0,
            percentage: 0.0,
            speed: 0.0,
            eta: 0.0,
            message: None,
            queue_position: None,
            queue_length: None,
            shard_progress: None,
        }
    }

    pub fn starting(model_id: &str) -> Self {
        let mut event = Self::base(model_id, "started");
        event.message = Some(format!("Starting download: {}", model_id));
        event
    }

    pub fn completed(model_id: &str, message: Option<&str>) -> Self {
        let mut event = Self::base(model_id, "completed");
        event.message = message.map(|m| m.to_string());
        event
    }

    pub fn errored(model_id: &str, message: &str) -> Self {
        let mut event = Self::base(model_id, "error");
        event.message = Some(message.to_string());
        event
    }

    /// Create a "queued" status event for items waiting in the download queue.
    pub fn queued(model_id: &str, position: usize, queue_length: usize) -> Self {
        let mut event = Self::base(model_id, "queued");
        event.queue_position = Some(position);
        event.queue_length = Some(queue_length);
        event.message = Some(format!(
            "Queued (position {} of {})",
            position, queue_length
        ));
        event
    }

    /// Create a "skipped" status event for downloads that failed and were skipped.
    pub fn skipped(model_id: &str, reason: &str) -> Self {
        let mut event = Self::base(model_id, "skipped");
        event.message = Some(format!("Skipped: {}", reason));
        event
    }

    /// Create a "paused" status event when the download queue is paused.
    pub fn paused(model_id: &str, downloaded: u64, total: u64) -> Self {
        let mut event = Self::base(model_id, "paused");
        event.downloaded = downloaded;
        event.total = total;
        event.percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        event.message = Some("Download paused".to_string());
        event
    }

    /// Create a "resumed" status event when the download queue is resumed.
    pub fn resumed(model_id: &str) -> Self {
        let mut event = Self::base(model_id, "resumed");
        event.message = Some("Download resumed".to_string());
        event
    }

    /// Create a "retry" status event when retrying after a network error.
    pub fn retry(model_id: &str, attempt: u32, max_attempts: u32, delay_secs: u64) -> Self {
        let mut event = Self::base(model_id, "retry");
        event.message = Some(format!(
            "Network error, retrying (attempt {}/{}) in {}s...",
            attempt, max_attempts, delay_secs
        ));
        event
    }

    /// Create a progress event with pre-calculated EWA speed.
    pub fn progress(model_id: &str, downloaded: u64, total: u64, speed: f64) -> Self {
        let mut event = Self::base(model_id, "progress");
        event.downloaded = downloaded;
        event.total = total;

        event.percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        event.speed = speed;

        event.eta = if speed > 0.0 && total > downloaded {
            (total - downloaded) as f64 / speed
        } else {
            0.0
        };

        event.message = Some(format!(
            "Downloading: {} ({:.1}%)",
            model_id, event.percentage
        ));
        event
    }

    /// Create a progress event with shard information for sharded model downloads.
    #[allow(clippy::too_many_arguments)]
    pub fn progress_with_shard(
        model_id: &str,
        shard_downloaded: u64,
        shard_total: u64,
        current_shard: usize,
        total_shards: usize,
        current_filename: &str,
        aggregate_downloaded: u64,
        aggregate_total: u64,
        speed: f64,
    ) -> Self {
        let mut event = Self::base(model_id, "progress");

        // Use aggregate values for the main progress
        event.downloaded = aggregate_downloaded;
        event.total = aggregate_total;

        event.percentage = if aggregate_total > 0 {
            (aggregate_downloaded as f64 / aggregate_total as f64) * 100.0
        } else {
            0.0
        };

        event.speed = speed;

        event.eta = if speed > 0.0 && aggregate_total > aggregate_downloaded {
            (aggregate_total - aggregate_downloaded) as f64 / speed
        } else {
            0.0
        };

        event.message = Some(format!(
            "Downloading shard {}/{}: {} ({:.1}%)",
            current_shard + 1,
            total_shards,
            model_id,
            event.percentage
        ));

        event.shard_progress = Some(ShardProgressInfo {
            current_shard,
            total_shards,
            current_filename: current_filename.to_string(),
            shard_downloaded,
            shard_total,
            aggregate_downloaded,
            aggregate_total,
        });

        event
    }

    /// Add queue information to an existing event.
    pub fn with_queue_info(mut self, position: usize, queue_length: usize) -> Self {
        self.queue_position = Some(position);
        self.queue_length = Some(queue_length);
        self
    }

    /// Add shard progress information to an existing event.
    pub fn with_shard_progress(mut self, shard_progress: ShardProgressInfo) -> Self {
        self.shard_progress = Some(shard_progress);
        self
    }
}

impl DownloadProgressEvent {
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::new())
    }
}

#[derive(Clone)]
pub struct ProgressThrottle {
    state: Arc<Mutex<ThrottleState>>,
    min_interval: Duration,
    min_step_bytes: u64,
}

struct ThrottleState {
    last_emit: Instant,
    last_bytes: u64,
    emitted: bool,
    /// Exponentially weighted average speed in bytes/sec
    ewa_speed: f64,
}

/// Smoothing factor for EWA speed calculation.
/// 0.02 = 2% weight to new sample, 98% to historical average.
/// This provides ~10 second response time for very stable speed display.
const EWA_SMOOTHING: f64 = 0.02;

impl ProgressThrottle {
    pub fn new(min_interval: Duration, min_step_bytes: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(ThrottleState {
                last_emit: Instant::now(),
                last_bytes: 0,
                emitted: false,
                ewa_speed: 0.0,
            })),
            min_interval,
            min_step_bytes,
        }
    }

    /// Tuned defaults for interactive UI progress bars (CLI+GUI).
    pub fn responsive_ui() -> Self {
        Self::new(Duration::from_millis(150), 512 * 1_024)
    }

    /// Check if we should emit a progress event, and if so, update EWA speed.
    /// Returns Some(ewa_speed) if should emit, None otherwise.
    pub fn should_emit_with_speed(&self, downloaded: u64, total: u64) -> Option<f64> {
        let mut state = self.state.lock().expect("progress throttle lock poisoned");
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_emit);
        let advanced = downloaded.saturating_sub(state.last_bytes);
        let force_emit = !state.emitted || (total > 0 && downloaded >= total);

        if !(force_emit || elapsed >= self.min_interval || advanced >= self.min_step_bytes) {
            return None;
        }

        // Calculate instantaneous speed for this interval
        let elapsed_secs = elapsed.as_secs_f64();
        let instant_speed = if elapsed_secs > 0.0 {
            advanced as f64 / elapsed_secs
        } else {
            0.0
        };

        // Update EWA speed: new_ewa = smoothing * instant + (1 - smoothing) * old_ewa
        // On first emit, just use the instant speed
        if state.emitted {
            state.ewa_speed =
                EWA_SMOOTHING * instant_speed + (1.0 - EWA_SMOOTHING) * state.ewa_speed;
        } else {
            state.ewa_speed = instant_speed;
        }

        state.last_emit = now;
        state.last_bytes = downloaded;
        state.emitted = true;
        Some(state.ewa_speed)
    }

    /// Legacy method for backwards compatibility - just returns bool.
    pub fn should_emit(&self, downloaded: u64, total: u64) -> bool {
        self.should_emit_with_speed(downloaded, total).is_some()
    }

    /// Get the current EWA speed without checking throttle.
    pub fn current_speed(&self) -> f64 {
        self.state
            .lock()
            .expect("progress throttle lock poisoned")
            .ewa_speed
    }
}
