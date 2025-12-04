//! Error types for the download module.

use thiserror::Error;

/// Errors that can occur during download operations.
#[derive(Debug, Error)]
pub enum DownloadError {
    /// Download was cancelled by the user.
    #[error("Download '{id}' was cancelled by the user")]
    Cancelled { id: String },

    /// Download is already running or queued.
    #[error("A download for '{id}' is already running or queued")]
    AlreadyQueued { id: String },

    /// Download not found (for cancel/status operations).
    #[error("No active download for '{id}'")]
    NotFound { id: String },

    /// Download queue is full.
    #[error("Download queue is full (max {max_size} items)")]
    QueueFull { max_size: u32 },

    /// Item not in queue.
    #[error("Item '{id}' not found in queue")]
    NotInQueue { id: String },

    /// Python environment setup failed.
    #[error("Python environment setup failed: {0}")]
    PythonEnvSetup(String),

    /// Python download process failed.
    #[error("Download process failed: {0}")]
    ProcessFailed(String),

    /// Download unavailable (e.g., hf_xet not available).
    #[error("Download unavailable: {0}")]
    Unavailable(String),

    /// HuggingFace API error.
    #[error("HuggingFace API error: {0}")]
    HuggingFaceApi(String),

    /// File not found on HuggingFace.
    #[error("No GGUF files found for quantization '{quantization}' in model '{model_id}'")]
    QuantizationNotFound {
        model_id: String,
        quantization: String,
    },

    /// Invalid model ID format.
    #[error("Invalid model ID: '{id}'")]
    InvalidModelId { id: String },

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error wrapper.
    #[error("{0}")]
    Other(String),
}

impl DownloadError {
    /// Create a cancelled error.
    pub fn cancelled(id: impl Into<String>) -> Self {
        Self::Cancelled { id: id.into() }
    }

    /// Create an already queued error.
    pub fn already_queued(id: impl Into<String>) -> Self {
        Self::AlreadyQueued { id: id.into() }
    }

    /// Create a not found error.
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound { id: id.into() }
    }

    /// Create a queue full error.
    pub fn queue_full(max_size: u32) -> Self {
        Self::QueueFull { max_size }
    }

    /// Create a not in queue error.
    pub fn not_in_queue(id: impl Into<String>) -> Self {
        Self::NotInQueue { id: id.into() }
    }

    /// Create an environment setup error.
    pub fn environment(message: impl Into<String>) -> Self {
        Self::PythonEnvSetup(message.into())
    }

    /// Create a process failed error.
    pub fn process_failed(message: impl Into<String>) -> Self {
        Self::ProcessFailed(message.into())
    }

    /// Create a quantization not found error.
    pub fn quantization_not_found(
        model_id: impl Into<String>,
        quantization: impl Into<String>,
    ) -> Self {
        Self::QuantizationNotFound {
            model_id: model_id.into(),
            quantization: quantization.into(),
        }
    }

    /// Check if this error represents a cancellation.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled { .. })
    }

    /// Check if this error represents a transient failure that could be retried.
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::ProcessFailed(_) | Self::HuggingFaceApi(_) | Self::Io(_)
        )
    }
}

impl From<anyhow::Error> for DownloadError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = DownloadError::cancelled("model:Q4_K_M");
        assert!(err.to_string().contains("cancelled"));
        assert!(err.to_string().contains("model:Q4_K_M"));
    }

    #[test]
    fn test_is_cancelled() {
        assert!(DownloadError::cancelled("test").is_cancelled());
        assert!(!DownloadError::not_found("test").is_cancelled());
    }

    #[test]
    fn test_is_retriable() {
        assert!(DownloadError::ProcessFailed("timeout".into()).is_retriable());
        assert!(DownloadError::HuggingFaceApi("500".into()).is_retriable());
        assert!(!DownloadError::cancelled("test").is_retriable());
        assert!(!DownloadError::queue_full(10).is_retriable());
    }
}
