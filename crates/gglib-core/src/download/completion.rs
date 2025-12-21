//! Queue run completion tracking types.
//!
//! These types represent the completion of an entire queue run, distinct from
//! individual download completion events. A queue run accumulates all downloads
//! that complete between idle→busy and busy→idle transitions.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use super::types::DownloadId;

/// Stable artifact identity for completion tracking.
///
/// This key is computed at enqueue time (before download starts) and remains
/// stable across retries, failures, and sharded downloads. It represents "what
/// the user thinks they downloaded" from an artifact perspective, not a request
/// perspective.
///
/// # Identity Semantics
///
/// - Same artifact downloaded twice → same key (deduplication)
/// - All shards in a group → same key (one entry)
/// - Failures before metadata available → key still valid
/// - Survives cancellations and retries
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompletionKey {
    /// `HuggingFace` model file.
    HfFile {
        /// Repository ID (e.g., "unsloth/Llama-3-GGUF").
        repo_id: String,
        /// Git revision (branch, tag, or commit SHA).
        /// Stores exactly what the user requested (e.g., "main", "v1.0", or a SHA).
        /// Use "unspecified" if no revision was provided.
        revision: String,
        /// Canonical filename (normalized for sharded models).
        /// Shard suffixes are stripped: "model-00001-of-00008.gguf" → "model.gguf"
        filename_canon: String,
        /// Quantization type (e.g., "`Q4_K_M`").
        /// Optional since some downloads may not have a meaningful quantization.
        #[serde(skip_serializing_if = "Option::is_none")]
        quantization: Option<String>,
    },

    /// File downloaded from URL.
    UrlFile {
        /// Source URL.
        url: String,
        /// Target filename.
        filename: String,
    },

    /// Local file operation.
    LocalFile {
        /// Absolute path to the file.
        path: String,
    },
}

impl fmt::Display for CompletionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HfFile {
                repo_id,
                quantization,
                ..
            } => {
                if let Some(quant) = quantization {
                    write!(f, "{repo_id} ({quant})")
                } else {
                    write!(f, "{repo_id}")
                }
            }
            Self::UrlFile { filename, .. } => write!(f, "{filename}"),
            Self::LocalFile { path } => {
                // Show only filename for local files
                if let Some(name) = path.rsplit('/').next() {
                    write!(f, "{name}")
                } else {
                    write!(f, "{path}")
                }
            }
        }
    }
}

/// Result kind for a completion attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionKind {
    /// Successfully downloaded and registered.
    Downloaded,
    /// Download failed.
    Failed,
    /// Download was cancelled by user.
    Cancelled,
    /// File already existed and was validated (not re-downloaded).
    AlreadyPresent,
}

/// Counts of attempts by result kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptCounts {
    /// Number of successful downloads.
    pub downloaded: u32,
    /// Number of failed attempts.
    pub failed: u32,
    /// Number of cancelled attempts.
    pub cancelled: u32,
}

impl AttemptCounts {
    /// Create counts with a single attempt of the given kind.
    #[must_use]
    pub const fn from_kind(kind: CompletionKind) -> Self {
        match kind {
            CompletionKind::Downloaded => Self {
                downloaded: 1,
                failed: 0,
                cancelled: 0,
            },
            CompletionKind::Failed => Self {
                downloaded: 0,
                failed: 1,
                cancelled: 0,
            },
            CompletionKind::Cancelled => Self {
                downloaded: 0,
                failed: 0,
                cancelled: 1,
            },
            CompletionKind::AlreadyPresent => Self {
                downloaded: 0,
                failed: 0,
                cancelled: 0,
            },
        }
    }

    /// Increment the count for the given kind.
    pub const fn increment(&mut self, kind: CompletionKind) {
        match kind {
            CompletionKind::Downloaded => self.downloaded += 1,
            CompletionKind::Failed => self.failed += 1,
            CompletionKind::Cancelled => self.cancelled += 1,
            CompletionKind::AlreadyPresent => {
                // AlreadyPresent doesn't increment attempt counts
                // (it's informational, not a retry)
            }
        }
    }

    /// Total number of attempts across all kinds.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.downloaded + self.failed + self.cancelled
    }

    /// Check if there were any retry attempts (more than one total attempt).
    #[must_use]
    pub const fn has_retries(&self) -> bool {
        self.total() > 1
    }
}

/// Details for a single completed artifact in a queue run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionDetail {
    /// Stable artifact identity key.
    pub key: CompletionKey,
    /// Human-readable display name for UI.
    pub display_name: String,
    /// Most recent result for this artifact.
    pub last_result: CompletionKind,
    /// Unix timestamp (milliseconds since epoch) of last completion.
    pub last_completed_at_ms: u64,
    /// All download IDs that contributed to this completion.
    /// Multiple IDs indicate retries or re-queues.
    pub download_ids: Vec<DownloadId>,
    /// Breakdown of attempts by result kind.
    pub attempt_counts: AttemptCounts,
}

/// Summary of an entire queue run from start to drain.
///
/// Emitted when the queue transitions from busy → idle, capturing all
/// completions that occurred during the run regardless of timing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRunSummary {
    /// Unique identifier for this queue run.
    pub run_id: Uuid,
    /// Unix timestamp (milliseconds since epoch) when the run started.
    pub started_at_ms: u64,
    /// Unix timestamp (milliseconds since epoch) when the run completed.
    pub completed_at_ms: u64,

    // Attempt-based totals (diagnostics)
    /// Total download attempts that succeeded.
    pub total_attempts_downloaded: u32,
    /// Total download attempts that failed.
    pub total_attempts_failed: u32,
    /// Total download attempts that were cancelled.
    pub total_attempts_cancelled: u32,

    // Unique key-based totals (UX)
    /// Number of unique models successfully downloaded.
    pub unique_models_downloaded: u32,
    /// Number of unique models that failed.
    pub unique_models_failed: u32,
    /// Number of unique models that were cancelled.
    pub unique_models_cancelled: u32,

    /// True if there are more items than shown in `items`.
    pub truncated: bool,

    /// Detailed completion records, sorted by `last_completed_at_ms` (newest first).
    /// Capped at 20 items for payload size management.
    pub items: Vec<CompletionDetail>,
}

impl QueueRunSummary {
    /// Total number of unique models across all result kinds.
    #[must_use]
    pub const fn total_unique_models(&self) -> u32 {
        self.unique_models_downloaded + self.unique_models_failed + self.unique_models_cancelled
    }

    /// Total number of attempts across all result kinds.
    #[must_use]
    pub const fn total_attempts(&self) -> u32 {
        self.total_attempts_downloaded + self.total_attempts_failed + self.total_attempts_cancelled
    }

    /// Check if any models had retry attempts.
    #[must_use]
    pub fn has_retries(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.attempt_counts.has_retries())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_key_display() {
        let key = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: Some("Q4_K_M".to_string()),
        };
        assert_eq!(key.to_string(), "unsloth/Llama-3-GGUF (Q4_K_M)");

        let key_no_quant = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: None,
        };
        assert_eq!(key_no_quant.to_string(), "unsloth/Llama-3-GGUF");
    }

    #[test]
    fn test_attempt_counts() {
        let mut counts = AttemptCounts::from_kind(CompletionKind::Downloaded);
        assert_eq!(counts.downloaded, 1);
        assert_eq!(counts.total(), 1);
        assert!(!counts.has_retries());

        counts.increment(CompletionKind::Failed);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.total(), 2);
        assert!(counts.has_retries());

        counts.increment(CompletionKind::Downloaded);
        assert_eq!(counts.downloaded, 2);
        assert_eq!(counts.total(), 3);
    }

    #[test]
    fn test_queue_run_summary_totals() {
        let summary = QueueRunSummary {
            run_id: Uuid::nil(),
            started_at_ms: 0,
            completed_at_ms: 100_000,
            total_attempts_downloaded: 5,
            total_attempts_failed: 1,
            total_attempts_cancelled: 0,
            unique_models_downloaded: 3,
            unique_models_failed: 1,
            unique_models_cancelled: 0,
            items: vec![],
            truncated: false,
        };

        assert_eq!(summary.total_attempts(), 6);
        assert_eq!(summary.total_unique_models(), 4);
    }
}
