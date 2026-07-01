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
    use crate::download::types::DownloadId;

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
    fn test_completion_key_display_variants() {
        // UrlFile displays the filename
        let url_key = CompletionKey::UrlFile {
            url: "https://example.com/model.gguf".to_string(),
            filename: "model.gguf".to_string(),
        };
        assert_eq!(url_key.to_string(), "model.gguf");

        // LocalFile with slash — shows only the basename
        let local_key = CompletionKey::LocalFile {
            path: "/home/user/models/llama-3.Q4_K_M.gguf".to_string(),
        };
        assert_eq!(local_key.to_string(), "llama-3.Q4_K_M.gguf");

        // LocalFile without slash — falls back to full path
        let local_no_slash = CompletionKey::LocalFile {
            path: "model.gguf".to_string(),
        };
        assert_eq!(local_no_slash.to_string(), "model.gguf");
    }

    #[test]
    fn test_completion_key_serde_roundtrip() {
        // HfFile with quantization — round-trip
        let hf_with_quant = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: Some("Q4_K_M".to_string()),
        };
        let json = serde_json::to_string(&hf_with_quant).unwrap();
        let parsed: CompletionKey = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, hf_with_quant);

        // HfFile without quantization — round-trip AND verify skip_serializing_if
        let hf_no_quant = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: None,
        };
        let json_no_quant = serde_json::to_string(&hf_no_quant).unwrap();
        let parsed_no_quant: CompletionKey = serde_json::from_str(&json_no_quant).unwrap();
        assert_eq!(parsed_no_quant, hf_no_quant);

        // Verify "quantization" key is ABSENT when None (not null)
        let value: serde_json::Value = serde_json::from_str(&json_no_quant).unwrap();
        assert!(
            value.get("quantization").is_none(),
            "quantization key should be absent when None, not present as null"
        );

        // Verify "quantization" key IS present when Some
        let value_with: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            value_with.get("quantization").is_some(),
            "quantization key should be present when Some"
        );

        // UrlFile round-trip
        let url_key = CompletionKey::UrlFile {
            url: "https://example.com/model.gguf".to_string(),
            filename: "model.gguf".to_string(),
        };
        let json_url = serde_json::to_string(&url_key).unwrap();
        let parsed_url: CompletionKey = serde_json::from_str(&json_url).unwrap();
        assert_eq!(parsed_url, url_key);

        // LocalFile round-trip
        let local_key = CompletionKey::LocalFile {
            path: "/home/user/models/llama.gguf".to_string(),
        };
        let json_local = serde_json::to_string(&local_key).unwrap();
        let parsed_local: CompletionKey = serde_json::from_str(&json_local).unwrap();
        assert_eq!(parsed_local, local_key);

        // CompletionKind snake_case wire format
        for (kind, expected_wire) in [
            (CompletionKind::Downloaded, "downloaded"),
            (CompletionKind::Failed, "failed"),
            (CompletionKind::Cancelled, "cancelled"),
            (CompletionKind::AlreadyPresent, "already_present"),
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert!(
                json.contains(expected_wire),
                "Expected CompletionKind {kind:?} to serialize to snake_case '{expected_wire}', got: {json}"
            );
            let parsed: CompletionKind = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn test_completion_key_hash_dedup() {
        use std::collections::HashSet;
        use std::hash::{Hash, Hasher};

        let key1 = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: Some("Q4_K_M".to_string()),
        };
        let key2 = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: Some("Q4_K_M".to_string()),
        };

        // Identical keys must be equal
        assert_eq!(key1, key2);

        // Identical keys must produce the same hash
        let mut h1 = std::collections::hash_map::DefaultHasher::new();
        let mut h2 = std::collections::hash_map::DefaultHasher::new();
        key1.hash(&mut h1);
        key2.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());

        // Insert duplicates into HashSet — should collapse to 1
        let mut set = HashSet::new();
        set.insert(key1);
        set.insert(key2.clone());
        assert_eq!(set.len(), 1);

        // Keys differing in revision are NOT equal (different revisions = distinct artifacts)
        let key_diff_revision = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "v2.0".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: Some("Q4_K_M".to_string()),
        };
        assert_ne!(key2, key_diff_revision);

        // Keys differing in filename_canon are NOT equal (different files = distinct)
        let key_diff_filename = CompletionKey::HfFile {
            repo_id: "unsloth/Llama-3-GGUF".to_string(),
            revision: "main".to_string(),
            filename_canon: "model-q8.gguf".to_string(),
            quantization: Some("Q4_K_M".to_string()),
        };
        assert_ne!(key2, key_diff_filename);
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
    fn test_attempt_counts_all_kinds() {
        // from_kind(Failed) — single failed attempt
        let failed = AttemptCounts::from_kind(CompletionKind::Failed);
        assert_eq!(failed.failed, 1);
        assert_eq!(failed.downloaded, 0);
        assert_eq!(failed.cancelled, 0);
        assert_eq!(failed.total(), 1);

        // from_kind(Cancelled) — single cancelled attempt
        let cancelled = AttemptCounts::from_kind(CompletionKind::Cancelled);
        assert_eq!(cancelled.cancelled, 1);
        assert_eq!(cancelled.downloaded, 0);
        assert_eq!(cancelled.failed, 0);
        assert_eq!(cancelled.total(), 1);

        // from_kind(AlreadyPresent) — produces all zeros, identical to Default
        let already = AttemptCounts::from_kind(CompletionKind::AlreadyPresent);
        let default_counts = AttemptCounts::default();
        assert_eq!(already, default_counts);
        assert_eq!(already.downloaded, 0);
        assert_eq!(already.failed, 0);
        assert_eq!(already.cancelled, 0);
        assert_eq!(already.total(), 0);

        // increment(Cancelled) — properly increments the cancelled counter
        let mut counts = AttemptCounts::default();
        counts.increment(CompletionKind::Cancelled);
        assert_eq!(counts.cancelled, 1);
        assert_eq!(counts.total(), 1);

        // increment(AlreadyPresent) — no-op: counts before == counts after
        let before = AttemptCounts {
            downloaded: 2,
            failed: 1,
            cancelled: 0,
        };
        let mut counts = before;
        counts.increment(CompletionKind::AlreadyPresent);
        assert_eq!(
            counts, before,
            "increment(AlreadyPresent) should be a no-op"
        );
    }

    #[test]
    fn test_completion_detail_serde_roundtrip() {
        // Construct a retry scenario: same model downloaded via two different DownloadIds
        // (first attempt failed, second succeeded).
        let id1 = DownloadId::from_model("llama-3");
        let id2 = DownloadId::new("unsloth/llama-3-gguf", Some("Q4_K_M"));

        let key = CompletionKey::HfFile {
            repo_id: "unsloth/llama-3-gguf".to_string(),
            revision: "main".to_string(),
            filename_canon: "model.gguf".to_string(),
            quantization: Some("Q4_K_M".to_string()),
        };

        let mut counts = AttemptCounts::default();
        counts.increment(CompletionKind::Failed);
        counts.increment(CompletionKind::Downloaded);

        let detail = CompletionDetail {
            key,
            display_name: "unsloth/llama-3-gguf (Q4_K_M)".to_string(),
            last_result: CompletionKind::Downloaded,
            last_completed_at_ms: 1_700_000_000_000,
            download_ids: vec![id1.clone(), id2.clone()],
            attempt_counts: counts,
        };

        // Serialize to JSON string
        let json = serde_json::to_string(&detail).expect("should serialize");

        // Deserialize back
        let restored: CompletionDetail = serde_json::from_str(&json).expect("should deserialize");

        // Full equality — all fields must match exactly
        assert_eq!(detail, restored);

        // Verify the retry scenario is preserved through the round-trip
        assert_eq!(restored.download_ids.len(), 2);
        assert_eq!(restored.download_ids[0], id1);
        assert_eq!(restored.download_ids[1], id2);
        assert_eq!(restored.attempt_counts.failed, 1);
        assert_eq!(restored.attempt_counts.downloaded, 1);
        assert_eq!(restored.attempt_counts.total(), 2);

        // Verify the JSON contains expected keys (wire-shape sanity)
        let value: serde_json::Value = serde_json::from_str(&json).expect("should parse as Value");
        assert!(value.get("download_ids").is_some());
        assert!(value.get("attempt_counts").is_some());
        assert!(value.get("key").is_some());
        assert!(value.get("display_name").is_some());
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

    #[test]
    fn test_queue_run_summary_has_retries() {
        // FALSE scenario: all items have exactly 1 attempt — no retries
        let single_attempt_detail = CompletionDetail {
            key: CompletionKey::HfFile {
                repo_id: "model-a".to_string(),
                revision: "main".to_string(),
                filename_canon: "a.gguf".to_string(),
                quantization: None,
            },
            display_name: "model-a".to_string(),
            last_result: CompletionKind::Downloaded,
            last_completed_at_ms: 1000,
            download_ids: vec![DownloadId::from_model("model-a")],
            attempt_counts: AttemptCounts::from_kind(CompletionKind::Downloaded),
        };

        let no_retry_summary = QueueRunSummary {
            run_id: Uuid::nil(),
            started_at_ms: 0,
            completed_at_ms: 1000,
            total_attempts_downloaded: 3,
            total_attempts_failed: 0,
            total_attempts_cancelled: 0,
            unique_models_downloaded: 3,
            unique_models_failed: 0,
            unique_models_cancelled: 0,
            items: vec![
                single_attempt_detail.clone(),
                single_attempt_detail.clone(),
                single_attempt_detail.clone(),
            ],
            truncated: false,
        };
        assert!(
            !no_retry_summary.has_retries(),
            "should be false when all items have exactly 1 attempt"
        );

        // TRUE scenario: one item has 3 attempts (2 failures + 1 success)
        let mut retried_counts = AttemptCounts::default();
        retried_counts.increment(CompletionKind::Failed);
        retried_counts.increment(CompletionKind::Failed);
        retried_counts.increment(CompletionKind::Downloaded);

        let retried_detail = CompletionDetail {
            key: CompletionKey::HfFile {
                repo_id: "model-b".to_string(),
                revision: "main".to_string(),
                filename_canon: "b.gguf".to_string(),
                quantization: None,
            },
            display_name: "model-b".to_string(),
            last_result: CompletionKind::Downloaded,
            last_completed_at_ms: 2000,
            download_ids: vec![
                DownloadId::from_model("model-b"),
                DownloadId::from_model("model-b"),
                DownloadId::from_model("model-b"),
            ],
            attempt_counts: retried_counts,
        };

        let retry_summary = QueueRunSummary {
            run_id: Uuid::nil(),
            started_at_ms: 0,
            completed_at_ms: 2000,
            // 3 unique models downloaded, but 5 total attempts (one was retried)
            total_attempts_downloaded: 4,
            total_attempts_failed: 2,
            total_attempts_cancelled: 0,
            unique_models_downloaded: 3,
            unique_models_failed: 0,
            unique_models_cancelled: 0,
            items: vec![
                single_attempt_detail.clone(),
                retried_detail.clone(),
                single_attempt_detail,
            ],
            truncated: false,
        };
        assert!(
            retry_summary.has_retries(),
            "should be true when at least one item has >1 attempt"
        );

        // Verify unique-vs-attempts distinction: 3 unique models but 6 total attempts
        assert_eq!(retry_summary.total_unique_models(), 3);
        assert_eq!(retry_summary.total_attempts(), 6);
        assert_ne!(
            retry_summary.total_unique_models(),
            retry_summary.total_attempts(),
            "unique models should differ from total attempts when retries occurred"
        );

        // Verify the retried item specifically
        assert_eq!(retried_detail.attempt_counts.failed, 2);
        assert_eq!(retried_detail.attempt_counts.downloaded, 1);
        assert_eq!(retried_detail.attempt_counts.total(), 3);
        assert!(retried_detail.attempt_counts.has_retries());
    }
}
