use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
        }
    }

    pub fn starting(model_id: &str) -> Self {
        let mut event = Self::base(model_id, "started");
        event.message = Some("Starting download...".to_string());
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

    pub fn progress(model_id: &str, downloaded: u64, total: u64, start_time: Instant) -> Self {
        let mut event = Self::base(model_id, "progress");
        event.downloaded = downloaded;
        event.total = total;

        event.percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let elapsed = start_time.elapsed().as_secs_f64();
        event.speed = if elapsed > 0.0 {
            downloaded as f64 / elapsed
        } else {
            0.0
        };

        event.eta = if event.speed > 0.0 && total > downloaded {
            (total - downloaded) as f64 / event.speed
        } else {
            0.0
        };

        event.message = Some(format!("Downloading... {:.1}%", event.percentage));
        event
    }

    /// Add queue information to an existing event.
    pub fn with_queue_info(mut self, position: usize, queue_length: usize) -> Self {
        self.queue_position = Some(position);
        self.queue_length = Some(queue_length);
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
}

impl ProgressThrottle {
    pub fn new(min_interval: Duration, min_step_bytes: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(ThrottleState {
                last_emit: Instant::now(),
                last_bytes: 0,
                emitted: false,
            })),
            min_interval,
            min_step_bytes,
        }
    }

    /// Tuned defaults for interactive UI progress bars (CLI+GUI).
    pub fn responsive_ui() -> Self {
        Self::new(Duration::from_millis(150), 512 * 1_024)
    }

    pub fn should_emit(&self, downloaded: u64, total: u64) -> bool {
        let mut state = self.state.lock().expect("progress throttle lock poisoned");
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_emit);
        let advanced = downloaded.saturating_sub(state.last_bytes);
        let force_emit = !state.emitted || (total > 0 && downloaded >= total);

        if !(force_emit || elapsed >= self.min_interval || advanced >= self.min_step_bytes) {
            return false;
        }

        state.last_emit = now;
        state.last_bytes = downloaded;
        state.emitted = true;
        true
    }
}
