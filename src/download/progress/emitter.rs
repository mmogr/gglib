//! Progress event emission for downloads.
//!
//! This module provides utilities for emitting download progress events
//! to the frontend via Tauri events or other channels.

use crate::download::domain::events::{DownloadEvent, DownloadStatus, DownloadSummary};
use crate::download::domain::types::ShardInfo;
use crate::download::progress::throttle::ProgressThrottle;
use crate::download::queue::QueueSnapshot;

/// Context for building progress events.
///
/// This struct holds the state needed to build consistent progress events
/// for a single download, including shard information and throttling.
#[derive(Clone)]
pub struct ProgressContext {
    /// Canonical ID string (model_id:quantization).
    pub id: String,
    /// Shard information if this is a sharded download.
    pub shard_info: Option<ShardInfo>,
    /// Total size across all shards (for aggregate progress).
    pub aggregate_total: u64,
    /// Size of already-completed shards.
    pub completed_shards_size: u64,
    /// Progress throttle.
    pub throttle: ProgressThrottle,
}

impl ProgressContext {
    /// Create a new progress context for a non-sharded download.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            shard_info: None,
            aggregate_total: 0,
            completed_shards_size: 0,
            throttle: ProgressThrottle::responsive_ui(),
        }
    }

    /// Create a new progress context for a sharded download.
    pub fn new_sharded(
        id: impl Into<String>,
        shard_info: ShardInfo,
        aggregate_total: u64,
        completed_shards_size: u64,
    ) -> Self {
        Self {
            id: id.into(),
            shard_info: Some(shard_info),
            aggregate_total,
            completed_shards_size,
            throttle: ProgressThrottle::responsive_ui(),
        }
    }

    /// Check if this is a sharded download.
    pub fn is_sharded(&self) -> bool {
        self.shard_info.is_some()
    }

    /// Build a progress event if throttle allows.
    ///
    /// Returns `Some(event)` if enough time/bytes have passed, `None` otherwise.
    pub fn build_progress(&self, downloaded: u64, total: u64) -> Option<DownloadEvent> {
        let speed = self.throttle.should_emit_with_speed(downloaded, total)?;

        if let Some(ref shard) = self.shard_info {
            // Sharded download
            let aggregate_downloaded = self.completed_shards_size + downloaded;
            let aggregate_total = if self.aggregate_total > 0 {
                self.aggregate_total
            } else {
                // Estimate based on current shard
                total * shard.total_shards as u64
            };

            Some(DownloadEvent::shard_progress(
                &self.id,
                shard.shard_index,
                shard.total_shards,
                &shard.filename,
                downloaded,
                total,
                aggregate_downloaded,
                aggregate_total,
                speed,
            ))
        } else {
            // Non-sharded download
            Some(DownloadEvent::progress(&self.id, downloaded, total, speed))
        }
    }

    /// Build a "started" event.
    pub fn build_started(&self) -> DownloadEvent {
        DownloadEvent::started(&self.id)
    }

    /// Build a "completed" event.
    pub fn build_completed(&self, message: Option<&str>) -> DownloadEvent {
        DownloadEvent::completed(&self.id, message)
    }

    /// Build a "failed" event.
    pub fn build_failed(&self, error: &str) -> DownloadEvent {
        DownloadEvent::failed(&self.id, error)
    }

    /// Build a "cancelled" event.
    pub fn build_cancelled(&self) -> DownloadEvent {
        DownloadEvent::cancelled(&self.id)
    }
}

/// Build a queue snapshot event from current queue state.
pub fn build_queue_snapshot(snapshot: &QueueSnapshot) -> DownloadEvent {
    DownloadEvent::queue_snapshot(snapshot.all_items(), snapshot.max_size)
}

/// Build a "queued" summary for a newly queued item.
pub fn build_queued_summary(
    id: impl Into<String>,
    display_name: impl Into<String>,
    position: u32,
    group_id: Option<String>,
    shard_info: Option<ShardInfo>,
) -> DownloadSummary {
    DownloadSummary {
        id: id.into(),
        display_name: display_name.into(),
        status: DownloadStatus::Queued,
        position,
        error: None,
        group_id,
        shard_info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_context_non_sharded() {
        let ctx = ProgressContext::new("model:Q4_K_M");
        assert!(!ctx.is_sharded());

        // First call should always emit
        let event = ctx.build_progress(500, 1000).unwrap();
        match event {
            DownloadEvent::DownloadProgress { id, downloaded, total, .. } => {
                assert_eq!(id, "model:Q4_K_M");
                assert_eq!(downloaded, 500);
                assert_eq!(total, 1000);
            }
            _ => panic!("Expected DownloadProgress"),
        }
    }

    #[test]
    fn test_progress_context_sharded() {
        let shard_info = ShardInfo::new(1, 3, "model-00002.gguf");
        let ctx = ProgressContext::new_sharded("model:Q4_K_M", shard_info, 3000, 1000);
        assert!(ctx.is_sharded());

        let event = ctx.build_progress(500, 1000).unwrap();
        match event {
            DownloadEvent::ShardProgress {
                id,
                shard_index,
                total_shards,
                aggregate_downloaded,
                aggregate_total,
                ..
            } => {
                assert_eq!(id, "model:Q4_K_M");
                assert_eq!(shard_index, 1);
                assert_eq!(total_shards, 3);
                assert_eq!(aggregate_downloaded, 1500); // 1000 completed + 500 current
                assert_eq!(aggregate_total, 3000);
            }
            _ => panic!("Expected ShardProgress"),
        }
    }

    #[test]
    fn test_build_queue_snapshot() {
        let snapshot = QueueSnapshot {
            current: None,
            pending: vec![],
            failed: vec![],
            max_size: 10,
        };

        let event = build_queue_snapshot(&snapshot);
        match event {
            DownloadEvent::QueueSnapshot { items, max_size } => {
                assert!(items.is_empty());
                assert_eq!(max_size, 10);
            }
            _ => panic!("Expected QueueSnapshot"),
        }
    }
}
